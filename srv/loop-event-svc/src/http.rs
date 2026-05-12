pub mod api;
pub mod middleware;
mod util;

use crate::http::api::user;
use axum::Router;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
pub use loop_infra::http_error::{ApiError as HttpErr, OptionExt, ResultExt};
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    pub exp: usize,

    pub user_id: i64,
    pub perm_ver: u32,
    pub role: String,
    pub patch_perm: PatchPerm,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PatchPerm {
    pub ban: Vec<String>,
    pub ext: Vec<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub jwt_enc: Arc<EncodingKey>,
    pub jwt_dec: Arc<DecodingKey>,
}

#[derive(Clone)]
pub struct AuthInfo {
    user_id: i64,
}

#[macro_export]
macro_rules! redis_conn {
    () => {{
        let pool = loop_infra::http_error::ResultExt::internal(
            loop_infra::redis::redis_pool(),
            "redis_pool_unavailable",
        )?;
        loop_infra::http_error::ResultExt::internal(pool.get().await, "redis_pool_get_failed")?
    }};
}

#[macro_export]
macro_rules! db_conn {
    () => {{
        let pool = loop_infra::http_error::ResultExt::internal(
            loop_infra::db::pg_pool(),
            "pg_pool_unavailable",
        )?;
        loop_infra::http_error::ResultExt::internal(pool.get().await, "pg_pool_get_failed")?
    }};
}

pub fn route(state: AppState) -> Router {
    Router::new().merge(user::route(state))
}

pub mod err_key {
    pub const ALREADY_EXIST: &str = "already_exist";
    pub const AUTH_CONTEXT_MISSING: &str = "auth_context_missing";
    pub const DB_ERROR: &str = "db_error";
    pub const EMAIL_SEND_ERROR: &str = "email_send_error";
    pub const INVALID_INPUT: &str = "invalid_input";
    pub const INVALID_REFRESH_TOKEN: &str = "invalid_refresh_token";
    pub const JWT_ENCODE_ERROR: &str = "jwt_encode_error";
    pub const PASSWORD_ERROR: &str = "password_error";
    pub const PASSWORD_HASH_ERROR: &str = "password_hash_error";
    pub const PASSWORD_VERIFY_ERROR: &str = "password_verify_error";
    pub const REDIS_ERROR: &str = "redis_error";
    pub const REFRESH_TOKEN_EXPIRED: &str = "refresh_token_expired";
    pub const TOO_MANY_REQUESTS: &str = "too_many_requests";
    pub const UNAUTHORIZED: &str = "unauthorized";
    pub const USER_NOT_EXIST: &str = "user_not_exist";
    pub const VERIFY_CODE_EXPIRED: &str = "verify_code_expired";
    pub const VERIFY_CODE_WRONG: &str = "verify_code_wrong";
}
