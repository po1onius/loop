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

use axum::{extract::FromRequestParts, http::request::Parts};

const PUBLIC_API_RULES: &[ApiRule] = &[
    ApiRule::new("POST", "/loop/user/login"),
    ApiRule::new("POST", "/loop/user/refresh_token"),
    ApiRule::new("POST", "/loop/user/register"),
    ApiRule::new("POST", "/loop/user/verify_code"),
];

const PROTECTED_API_RULES: &[ApiPermissionRule] = &[
    // ApiPermissionRule {
    //     rule: ApiRule::new("POST", "/loop/event"),
    //     permission: "event.create",
    // },
];

#[derive(Clone, Copy)]
struct ApiRule {
    method: &'static str,
    path: &'static str,
}

impl ApiRule {
    const fn new(method: &'static str, path: &'static str) -> Self {
        Self { method, path }
    }

    fn matches(self, method: &Method, path: &str) -> bool {
        self.method == method.as_str() && self.path == path
    }
}

#[derive(Clone, Copy)]
struct ApiPermissionRule {
    rule: ApiRule,
    permission: &'static str,
}

impl ApiPermissionRule {
    fn matches(self, method: &Method, path: &str) -> bool {
        self.rule.matches(method, path)
    }
}

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

#[tracing::instrument(name = "auth.middleware", skip_all)]
pub async fn auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response<Body>, HttpErr> {
    let requirement = permission_requirement(&req);
    let claims = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|t| t.strip_prefix("Bearer "))
        .and_then(|t| {
            let mut validation = Validation::new(Algorithm::RS256);
            validation.validate_aud = false;
            validation.leeway = 0;
            decode::<Claims>(t, &state.jwt_dec, &validation).ok()
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
            if let Some((_claims, auth_info)) = claims {
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
            let (claims, auth_info) = claims.client(StatusCode::UNAUTHORIZED, UNAUTHORIZED)?;
            authorize_claims(&claims, permission)?;
            tracing::debug!(
                user.id = auth_info.user_id,
                auth.role = %auth_info.role,
                auth.permission = permission,
                "authorized protected request"
            );
            req.extensions_mut().insert(auth_info);
        }
        PermissionRequirement::Unconfigured => {
            let path = request_path(&req);
            tracing::warn!(
                http.method = req.method().as_str(),
                http.route = %path,
                "api permission is not configured"
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

fn permission_requirement(req: &Request) -> PermissionRequirement {
    let method = req.method();
    let path = request_path(req);

    if PUBLIC_API_RULES
        .iter()
        .any(|rule| rule.matches(method, path.as_ref()))
    {
        return PermissionRequirement::Public;
    }

    PROTECTED_API_RULES
        .iter()
        .find(|rule| rule.matches(method, path.as_ref()))
        .map(|rule| PermissionRequirement::Protected(rule.permission))
        .unwrap_or(PermissionRequirement::Unconfigured)
}

fn request_path(req: &Request) -> std::borrow::Cow<'_, str> {
    req.extensions()
        .get::<MatchedPath>()
        .map(|path| std::borrow::Cow::Borrowed(path.as_str()))
        .unwrap_or_else(|| std::borrow::Cow::Borrowed(req.uri().path()))
}

fn authorize_claims(claims: &Claims, permission: &str) -> Result<(), HttpErr> {
    let cfg = CONFIG.load();
    if claims.perm_ver < cfg.perm.perm_ver {
        tracing::warn!(
            user.id = claims.user_id,
            token.perm_ver = claims.perm_ver,
            config.perm_ver = cfg.perm.perm_ver,
            "token permission version is stale"
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
        user.id = claims.user_id,
        auth.role = %claims.role,
        auth.permission = permission,
        "permission denied"
    );
    Err(HttpErr::client(StatusCode::FORBIDDEN, PERMISSION_DENIED))
}

fn has_permission(permissions: &[String], required: &str) -> bool {
    permissions
        .iter()
        .any(|permission| permission_matches(permission, required))
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
    fn role_permission_allows_matching_permission() {
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
