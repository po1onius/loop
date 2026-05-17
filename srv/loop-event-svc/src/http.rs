pub mod api;
pub mod middleware;
mod util;

use crate::http::{
    api::{event, media, user},
    middleware::{ApiRule, RouteAccess},
};
use axum::{Router, routing::MethodRouter};
use http::Method;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use loop_infra::http_error::ApiErrorCode;
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
}

#[derive(Clone)]
pub struct AppState {
    pub jwt_enc: Arc<EncodingKey>,
    pub jwt_dec: Arc<DecodingKey>,
}

#[derive(Clone)]
pub struct AuthInfo {
    pub user_id: i64,
    pub role: String,
}

// Keeps Axum route registration and auth policy registration on the same path.
// Each API module returns this wrapper instead of a bare Router.
pub struct AppRoutes<S = ()> {
    router: Router<S>,
    rules: Vec<ApiRule>,
}

impl AppRoutes {
    pub fn empty() -> Self {
        Self {
            router: Router::new(),
            rules: Vec::new(),
        }
    }

    pub fn nest(mut self, path: &'static str, routes: AppRoutes) -> Self {
        self.router = self.router.nest(path, routes.router);
        self.rules
            .extend(routes.rules.into_iter().map(|rule| rule.with_prefix(path)));
        self
    }

    pub fn merge(mut self, routes: AppRoutes) -> Self {
        self.router = self.router.merge(routes.router);
        self.rules.extend(routes.rules);
        self
    }

    pub fn into_parts(self) -> (Router, Vec<ApiRule>) {
        (self.router, self.rules)
    }
}

impl<S> AppRoutes<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            router: Router::new(),
            rules: Vec::new(),
        }
    }

    pub fn public(
        self,
        method: Method,
        path: &'static str,
        method_router: MethodRouter<S>,
    ) -> Self {
        self.route(method, path, RouteAccess::Public, method_router)
    }

    pub fn protected(
        self,
        method: Method,
        path: &'static str,
        permission: &'static str,
        method_router: MethodRouter<S>,
    ) -> Self {
        self.route(
            method,
            path,
            RouteAccess::Protected(permission),
            method_router,
        )
    }

    fn route(
        mut self,
        method: Method,
        path: &'static str,
        access: RouteAccess,
        method_router: MethodRouter<S>,
    ) -> Self {
        self.router = self.router.route(path, method_router);
        self.rules.push(ApiRule::new(method, path, access));
        self
    }

    pub fn with_state(self, state: S) -> AppRoutes {
        AppRoutes {
            router: self.router.with_state(state),
            rules: self.rules,
        }
    }
}

impl<S> Default for AppRoutes<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[macro_export]
macro_rules! redis_conn {
    () => {{
        let pool = loop_infra::http_error::ResultExt::internal(
            loop_infra::redis::redis_pool(),
            $crate::http::err_key::REDIS_POOL_UNAVAILABLE,
        )?;
        loop_infra::http_error::ResultExt::internal(
            pool.get().await,
            $crate::http::err_key::REDIS_POOL_GET_FAILED,
        )?
    }};
}

#[macro_export]
macro_rules! db_conn {
    () => {{
        let pool = loop_infra::http_error::ResultExt::internal(
            loop_infra::db::pg_pool(),
            $crate::http::err_key::PG_POOL_UNAVAILABLE,
        )?;
        loop_infra::http_error::ResultExt::internal(
            pool.get().await,
            $crate::http::err_key::PG_POOL_GET_FAILED,
        )?
    }};
}

pub fn route(state: AppState) -> AppRoutes {
    AppRoutes::empty().nest(
        "/loop",
        AppRoutes::empty()
            .merge(user::route(state.clone()))
            .merge(event::route(state.clone()))
            .merge(media::route(state)),
    )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorCode {
    AlreadyExist,
    AuthContextMissing,
    DbError,
    EmailSendError,
    InvalidInput,
    InvalidRefreshToken,
    JwtEncodeError,
    MediaNotFound,
    PasswordError,
    PasswordHashError,
    PasswordVerifyError,
    PermissionDenied,
    PermissionUnconfigured,
    PgPoolGetFailed,
    PgPoolUnavailable,
    RedisError,
    RedisPoolGetFailed,
    RedisPoolUnavailable,
    RefreshTokenExpired,
    StalePermission,
    StorageError,
    StorageUnconfigured,
    TooManyRequests,
    Unauthorized,
    UserNotExist,
    VerifyCodeExpired,
    VerifyCodeWrong,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyExist => "already_exist",
            Self::AuthContextMissing => "auth_context_missing",
            Self::DbError => "db_error",
            Self::EmailSendError => "email_send_error",
            Self::InvalidInput => "invalid_input",
            Self::InvalidRefreshToken => "invalid_refresh_token",
            Self::JwtEncodeError => "jwt_encode_error",
            Self::MediaNotFound => "media_not_found",
            Self::PasswordError => "password_error",
            Self::PasswordHashError => "password_hash_error",
            Self::PasswordVerifyError => "password_verify_error",
            Self::PermissionDenied => "permission_denied",
            Self::PermissionUnconfigured => "permission_unconfigured",
            Self::PgPoolGetFailed => "pg_pool_get_failed",
            Self::PgPoolUnavailable => "pg_pool_unavailable",
            Self::RedisError => "redis_error",
            Self::RedisPoolGetFailed => "redis_pool_get_failed",
            Self::RedisPoolUnavailable => "redis_pool_unavailable",
            Self::RefreshTokenExpired => "refresh_token_expired",
            Self::StalePermission => "stale_permission",
            Self::StorageError => "storage_error",
            Self::StorageUnconfigured => "storage_unconfigured",
            Self::TooManyRequests => "too_many_requests",
            Self::Unauthorized => "unauthorized",
            Self::UserNotExist => "user_not_exist",
            Self::VerifyCodeExpired => "verify_code_expired",
            Self::VerifyCodeWrong => "verify_code_wrong",
        }
    }
}

impl From<ErrorCode> for ApiErrorCode {
    fn from(code: ErrorCode) -> Self {
        Self::new(code.as_str())
    }
}

pub mod err_key {
    use super::ErrorCode;

    pub const ALREADY_EXIST: ErrorCode = ErrorCode::AlreadyExist;
    pub const AUTH_CONTEXT_MISSING: ErrorCode = ErrorCode::AuthContextMissing;
    pub const DB_ERROR: ErrorCode = ErrorCode::DbError;
    pub const EMAIL_SEND_ERROR: ErrorCode = ErrorCode::EmailSendError;
    pub const INVALID_INPUT: ErrorCode = ErrorCode::InvalidInput;
    pub const INVALID_REFRESH_TOKEN: ErrorCode = ErrorCode::InvalidRefreshToken;
    pub const JWT_ENCODE_ERROR: ErrorCode = ErrorCode::JwtEncodeError;
    pub const MEDIA_NOT_FOUND: ErrorCode = ErrorCode::MediaNotFound;
    pub const PASSWORD_ERROR: ErrorCode = ErrorCode::PasswordError;
    pub const PASSWORD_HASH_ERROR: ErrorCode = ErrorCode::PasswordHashError;
    pub const PASSWORD_VERIFY_ERROR: ErrorCode = ErrorCode::PasswordVerifyError;
    pub const PERMISSION_DENIED: ErrorCode = ErrorCode::PermissionDenied;
    pub const PERMISSION_UNCONFIGURED: ErrorCode = ErrorCode::PermissionUnconfigured;
    pub const PG_POOL_GET_FAILED: ErrorCode = ErrorCode::PgPoolGetFailed;
    pub const PG_POOL_UNAVAILABLE: ErrorCode = ErrorCode::PgPoolUnavailable;
    pub const REDIS_ERROR: ErrorCode = ErrorCode::RedisError;
    pub const REDIS_POOL_GET_FAILED: ErrorCode = ErrorCode::RedisPoolGetFailed;
    pub const REDIS_POOL_UNAVAILABLE: ErrorCode = ErrorCode::RedisPoolUnavailable;
    pub const REFRESH_TOKEN_EXPIRED: ErrorCode = ErrorCode::RefreshTokenExpired;
    pub const STALE_PERMISSION: ErrorCode = ErrorCode::StalePermission;
    pub const STORAGE_ERROR: ErrorCode = ErrorCode::StorageError;
    pub const STORAGE_UNCONFIGURED: ErrorCode = ErrorCode::StorageUnconfigured;
    pub const TOO_MANY_REQUESTS: ErrorCode = ErrorCode::TooManyRequests;
    pub const UNAUTHORIZED: ErrorCode = ErrorCode::Unauthorized;
    pub const USER_NOT_EXIST: ErrorCode = ErrorCode::UserNotExist;
    pub const VERIFY_CODE_EXPIRED: ErrorCode = ErrorCode::VerifyCodeExpired;
    pub const VERIFY_CODE_WRONG: ErrorCode = ErrorCode::VerifyCodeWrong;
}
