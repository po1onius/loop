use crate::{
    config::CONFIG,
    http::{AppState, AuthInfo, Claims},
};
use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use srv_common::http::{HttpErr, auth_check};
use tower_cookies::Cookies;

pub async fn auth(
    State(state): State<AppState>,
    cookies: Cookies,
    req: Request,
    next: Next,
) -> Result<Response<Body>, HttpErr> {
    let cfg = CONFIG.load();
    let whith_list = cfg
        .whith_list_api
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>();
    auth_check(
        cookies,
        req,
        next,
        &state.jwt_dec,
        &whith_list,
        "auth",
        |c: Claims| AuthInfo { user_id: c.user_id },
    )
    .await
}
