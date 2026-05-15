use crate::{
    config::CONFIG,
    http::{AppState, AuthInfo, Claims, HttpErr, OptionExt, err_key::*},
};
use axum::{
    body::Body,
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use http::{Method, StatusCode, header::AUTHORIZATION};
use jsonwebtoken::{Algorithm, Validation, decode};
use std::sync::Arc;

use axum::{extract::FromRequestParts, http::request::Parts};

#[derive(Clone)]
pub struct AuthMiddlewareState {
    app: AppState,
    policy: AuthPolicy,
}

impl AuthMiddlewareState {
    pub fn new(app: AppState, policy: AuthPolicy) -> Self {
        Self { app, policy }
    }
}

#[derive(Clone, Debug)]
pub struct AuthPolicy {
    rules: Arc<[ApiRule]>,
}

impl AuthPolicy {
    pub fn new(rules: Vec<ApiRule>) -> Self {
        Self {
            rules: Arc::from(rules),
        }
    }

    fn requirement(&self, method: &Method, path: &str) -> PermissionRequirement {
        self.rules
            .iter()
            .find(|rule| rule.matches(method, path))
            .map(|rule| match rule.access {
                RouteAccess::Public => PermissionRequirement::Public,
                RouteAccess::Protected(permission) => PermissionRequirement::Protected(permission),
            })
            .unwrap_or(PermissionRequirement::Unconfigured)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiRule {
    method: Method,
    path: String,
    access: RouteAccess,
}

impl ApiRule {
    pub fn new(method: Method, path: impl Into<String>, access: RouteAccess) -> Self {
        Self {
            method,
            path: normalize_route_path(path.into()),
            access,
        }
    }

    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.path = join_route_path(prefix, &self.path);
        self
    }

    fn matches(&self, method: &Method, path: &str) -> bool {
        self.method.as_str() == method.as_str() && self.path == path
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteAccess {
    Public,
    Protected(&'static str),
}

#[derive(Debug, Eq, PartialEq)]
enum PermissionRequirement {
    Public,
    Protected(&'static str),
    Unconfigured,
}

impl<S> FromRequestParts<S> for AuthInfo
where
    S: Send + Sync,
{
    type Rejection = HttpErr;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthInfo>()
            .cloned()
            .client(StatusCode::UNAUTHORIZED, AUTH_CONTEXT_MISSING)
    }
}

#[tracing::instrument(
    name = "auth.middleware",
    skip_all,
    fields(
        http.method = tracing::field::Empty,
        http.route = tracing::field::Empty,
        user.id = tracing::field::Empty,
        auth.role = tracing::field::Empty,
        auth.permission = tracing::field::Empty,
    )
)]
pub async fn auth(
    State(state): State<AuthMiddlewareState>,
    mut req: Request,
    next: Next,
) -> Result<Response<Body>, HttpErr> {
    let span = tracing::Span::current();
    let route = request_path(&req).into_owned();
    span.record("http.method", tracing::field::display(req.method()));
    span.record("http.route", tracing::field::display(&route));

    let requirement = state.policy.requirement(req.method(), &route);
    let claims = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|t| t.strip_prefix("Bearer "))
        .and_then(|t| {
            let mut validation = Validation::new(Algorithm::RS256);
            validation.validate_aud = false;
            validation.leeway = 0;
            decode::<Claims>(t, &state.app.jwt_dec, &validation).ok()
        })
        .map(|c| c.claims)
        .map(|claims| {
            let auth_info = AuthInfo {
                user_id: claims.user_id,
                role: claims.role.clone(),
            };
            (claims, auth_info)
        });

    match requirement {
        PermissionRequirement::Public => {
            span.record("auth.permission", tracing::field::display("public"));
            if let Some((_claims, auth_info)) = claims {
                span.record("user.id", auth_info.user_id);
                span.record("auth.role", tracing::field::display(&auth_info.role));
                tracing::debug!(
                    user.id = auth_info.user_id,
                    auth.role = %auth_info.role,
                    "authenticated public request"
                );
                req.extensions_mut().insert(auth_info);
            } else {
                tracing::debug!("public request without auth context");
            }
        }
        PermissionRequirement::Protected(permission) => {
            span.record("auth.permission", tracing::field::display(permission));
            let (claims, auth_info) = claims.client(StatusCode::UNAUTHORIZED, UNAUTHORIZED)?;
            authorize_claims(&claims, permission)?;
            span.record("user.id", auth_info.user_id);
            span.record("auth.role", tracing::field::display(&auth_info.role));
            tracing::debug!(
                user.id = auth_info.user_id,
                auth.role = %auth_info.role,
                auth.permission = permission,
                "authorized protected request"
            );
            req.extensions_mut().insert(auth_info);
        }
        PermissionRequirement::Unconfigured => {
            span.record("auth.permission", tracing::field::display("unconfigured"));
            let path = request_path(&req);
            tracing::warn!(
                event = "auth.permission_unconfigured",
                http_method = req.method().as_str(),
                http_route = %path,
                "API permission is not configured for {} {}",
                req.method(),
                path,
            );
            return Err(HttpErr::client(
                StatusCode::FORBIDDEN,
                PERMISSION_UNCONFIGURED,
            ));
        }
    }

    let res = next.run(req).await;
    Ok(res)
}

fn request_path(req: &Request) -> std::borrow::Cow<'_, str> {
    req.extensions()
        .get::<MatchedPath>()
        .map(|path| std::borrow::Cow::Borrowed(path.as_str()))
        .unwrap_or_else(|| std::borrow::Cow::Borrowed(req.uri().path()))
}

#[tracing::instrument(
    name = "auth.authorize_claims",
    skip_all,
    fields(
        user.id = claims.user_id,
        auth.role = %claims.role,
        auth.permission = %permission,
        token.perm_ver = claims.perm_ver,
    )
)]
fn authorize_claims(claims: &Claims, permission: &str) -> Result<(), HttpErr> {
    let cfg = CONFIG.load();
    if claims.perm_ver < cfg.perm.perm_ver {
        tracing::warn!(
            event = "auth.stale_permission",
            user_id = claims.user_id,
            token_perm_ver = claims.perm_ver,
            config_perm_ver = cfg.perm.perm_ver,
            "token permission version is stale for user {}",
            claims.user_id,
        );
        return Err(HttpErr::client(StatusCode::UNAUTHORIZED, STALE_PERMISSION));
    }

    if cfg
        .perm
        .role_perm
        .get(&claims.role)
        .is_some_and(|permissions| has_permission(permissions, permission))
    {
        return Ok(());
    }

    tracing::warn!(
        event = "auth.permission_denied",
        user_id = claims.user_id,
        auth_role = %claims.role,
        auth_permission = permission,
        "permission denied for user {} role {} permission {}",
        claims.user_id,
        claims.role,
        permission,
    );
    Err(HttpErr::client(StatusCode::FORBIDDEN, PERMISSION_DENIED))
}

fn has_permission(permissions: &[String], required: &str) -> bool {
    permissions
        .iter()
        .any(|permission| permission_matches(permission, required))
}

fn normalize_route_path(path: String) -> String {
    if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    }
}

fn join_route_path(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    if prefix.is_empty() {
        normalize_route_path(path.to_string())
    } else if path.is_empty() {
        normalize_route_path(prefix.to_string())
    } else {
        format!("{prefix}/{path}")
    }
}

fn permission_matches(permission: &str, required: &str) -> bool {
    permission == "*"
        || permission == required
        || permission.strip_suffix(".*").is_some_and(|prefix| {
            required.starts_with(prefix) && required[prefix.len()..].starts_with('.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static CONFIG_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn exact_permission_matches() {
        assert!(permission_matches("event.create", "event.create"));
        assert!(!permission_matches("event.create", "event.delete"));
    }

    #[test]
    fn wildcard_permission_matches_children_only() {
        assert!(permission_matches("event.*", "event.create"));
        assert!(permission_matches("event.*", "event.admin.delete"));
        assert!(!permission_matches("event.*", "event"));
        assert!(!permission_matches("event.*", "eventual.create"));
    }

    #[test]
    fn global_wildcard_matches_everything() {
        assert!(permission_matches("*", "community.post.audit"));
    }

    #[test]
    fn auth_policy_resolves_route_requirements() {
        let policy = AuthPolicy::new(vec![
            ApiRule::new(Method::POST, "/loop/user/login", RouteAccess::Public),
            ApiRule::new(
                Method::POST,
                "/loop/event",
                RouteAccess::Protected("event.create"),
            ),
        ]);

        assert_eq!(
            policy.requirement(&Method::POST, "/loop/user/login"),
            PermissionRequirement::Public
        );
        assert_eq!(
            policy.requirement(&Method::POST, "/loop/event"),
            PermissionRequirement::Protected("event.create")
        );
        assert_eq!(
            policy.requirement(&Method::GET, "/loop/user/login"),
            PermissionRequirement::Unconfigured
        );
        assert_eq!(
            policy.requirement(&Method::POST, "/loop/unregistered"),
            PermissionRequirement::Unconfigured
        );
    }

    #[test]
    fn role_permission_allows_matching_permission() {
        let _guard = CONFIG_TEST_LOCK.lock().expect("config test lock poisoned");
        let claims = Claims {
            exp: usize::MAX,
            user_id: 7,
            perm_ver: 1,
            role: "organizer".to_string(),
        };
        CONFIG.store(std::sync::Arc::new(crate::config::Config {
            access_ttl: 900,
            refresh_ttl: 3600,
            perm: crate::config::Perm {
                perm_ver: 1,
                role_perm: std::collections::HashMap::from([(
                    "organizer".to_string(),
                    vec!["event.create".to_string()],
                )]),
            },
            ..Default::default()
        }));

        authorize_claims(&claims, "event.create").expect("permission should be allowed");
    }

    #[test]
    fn role_permission_denies_missing_permission() {
        let _guard = CONFIG_TEST_LOCK.lock().expect("config test lock poisoned");
        let claims = Claims {
            exp: usize::MAX,
            user_id: 7,
            perm_ver: 1,
            role: "user".to_string(),
        };
        CONFIG.store(std::sync::Arc::new(crate::config::Config {
            access_ttl: 900,
            refresh_ttl: 3600,
            perm: crate::config::Perm {
                perm_ver: 1,
                role_perm: std::collections::HashMap::from([(
                    "user".to_string(),
                    vec!["event.read".to_string()],
                )]),
            },
            ..Default::default()
        }));

        let err = authorize_claims(&claims, "event.delete").expect_err("permission should deny");
        match err {
            HttpErr::Client { status, code, .. } => {
                assert_eq!(status, StatusCode::FORBIDDEN);
                assert_eq!(code.as_str(), PERMISSION_DENIED.as_str());
            }
            HttpErr::Internal { .. } => panic!("expected client error"),
        }
    }
}
