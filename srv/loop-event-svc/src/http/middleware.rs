use crate::{
    config::CONFIG,
    http::{AppState, AuthInfo, Claims, HttpErr, OptionExt, err_key::*},
};
use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use http::{StatusCode, header::AUTHORIZATION};
use jsonwebtoken::{Algorithm, Validation, decode};

use axum::{extract::FromRequestParts, http::request::Parts};

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
    let auth_info = req
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
        .map(|c| AuthInfo { user_id: c.user_id });

    let route = req.uri().to_string();
    tracing::debug!("route: {}", route);

    if let Some(auth_info) = auth_info {
        tracing::debug!(user.id = auth_info.user_id, "authenticated request");
        let ext = req.extensions_mut();
        ext.insert(auth_info);
    } else if CONFIG.load().whith_list_api.contains(&route) {
        tracing::debug!("white list");
    } else {
        tracing::debug!("UNAUTHORIZED");
        return Err(HttpErr::client(StatusCode::UNAUTHORIZED, UNAUTHORIZED));
    }

    let res = next.run(req).await;
    Ok(res)
}
