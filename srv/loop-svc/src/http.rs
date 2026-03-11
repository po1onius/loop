pub mod api;
pub mod middleware;
mod util;

use crate::http::api::user;
use axum::Router;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::{body::Body, response::Response};
use diesel::result::DatabaseErrorKind;
use http::StatusCode;
use http::header::AUTHORIZATION;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use std::panic::Location;
use std::sync::Arc;
use thiserror::Error;
use tower_cookies::{CookieManagerLayer, Cookies};

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
    () => {
        $crate::http::InnerExceptionHandle::ieh(
            $crate::infra::REDIS_POOL.get().expect("").get().await,
        )?
    };
}

#[macro_export]
macro_rules! db_conn {
    () => {
        $crate::http::InnerExceptionHandle::ieh(
            $crate::infra::PG_POOL.get().expect("").get().await,
        )?
    };
}

pub fn build_resp(code: StatusCode, content: &str) -> Response {
    let mut r = Response::new(Body::new(content.to_string()));
    *r.status_mut() = code;
    r
}

#[derive(Error, Debug)]
pub enum HttpErr {
    #[error("{1}")]
    ClientErr(StatusCode, String),
    #[error("server error")]
    InnerErr,
}

impl IntoResponse for HttpErr {
    fn into_response(self) -> Response {
        match self {
            Self::ClientErr(code, msg) => build_resp(code, &msg),
            Self::InnerErr => build_resp(StatusCode::INTERNAL_SERVER_ERROR, &self.to_string()),
        }
    }
}

fn convert_err<T, E: Display>(
    r: Result<T, E>,
    span: &Location,
    log: Option<&str>,
    code: Option<StatusCode>,
) -> Result<T, HttpErr> {
    r.map_err(|e| {
        tracing::error!("{} => {}", span, e);
        if let Some(log) = log
            && let Some(code) = code
        {
            HttpErr::ClientErr(code, log.to_string())
        } else {
            HttpErr::InnerErr
        }
    })
}

pub trait InnerExceptionHandle {
    type Output;
    fn ieh(self) -> Self::Output;
}

impl<T, E: Display> InnerExceptionHandle for Result<T, E> {
    type Output = Result<T, HttpErr>;
    #[track_caller]
    fn ieh(self) -> Self::Output {
        let caller_span = std::panic::Location::caller();
        convert_err(self, caller_span, None, None)
    }
}

pub trait ExceptionHandle {
    type Output;
    fn eh(self, code: StatusCode, err_info: &str) -> Self::Output;
}

impl<T, E: Display> ExceptionHandle for Result<T, E> {
    type Output = Result<T, HttpErr>;
    #[track_caller]
    fn eh(self, code: StatusCode, err_info: &str) -> Self::Output {
        let caller_span = std::panic::Location::caller();
        convert_err(self, caller_span, Some(err_info), Some(code))
    }
}

impl<T> InnerExceptionHandle for Option<T> {
    type Output = Result<T, HttpErr>;
    #[track_caller]
    fn ieh(self) -> Self::Output {
        let caller_span = std::panic::Location::caller();
        convert_err(self.ok_or("data is none"), caller_span, None, None)
    }
}

impl<T> ExceptionHandle for Option<T> {
    type Output = Result<T, HttpErr>;
    #[track_caller]
    fn eh(self, code: StatusCode, err_info: &str) -> Self::Output {
        let caller_span = std::panic::Location::caller();
        convert_err(
            self.ok_or("data is none"),
            caller_span,
            Some(err_info),
            Some(code),
        )
    }
}

impl From<diesel::result::Error> for HttpErr {
    fn from(value: diesel::result::Error) -> Self {
        match value {
            diesel::result::Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => {
                HttpErr::ClientErr(StatusCode::CONFLICT, "data already exist".to_string())
            }
            _ => HttpErr::InnerErr,
        }
    }
}

pub async fn http_serve(mut router: Router, port: u32) {
    router = router.layer(CookieManagerLayer::new());
    #[cfg(debug_assertions)]
    {
        use ::http::{HeaderValue, Method};
        use tower_http::cors::{AllowHeaders, CorsLayer};
        tracing::debug!("debug mode: cors layer");
        let cors_layzer = CorsLayer::new()
            .allow_headers(AllowHeaders::mirror_request())
            .allow_origin("http://127.0.0.1:3000".parse::<HeaderValue>().unwrap())
            .allow_methods([
                Method::OPTIONS,
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
            ])
            .allow_credentials(true);
        router = router.layer(cors_layzer);
    }

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .unwrap();
    tracing::info!("service start");
    axum::serve(listener, router).await.unwrap();
}

pub async fn auth_check<
    T: for<'a> Deserialize<'a>,
    U: Clone + Send + 'static + Sync,
    F: Fn(T) -> U,
>(
    cookies: Cookies,
    mut req: Request,
    next: Next,
    jwt_rsa_de_pk: &DecodingKey,
    white_list: &[&str],
    cookie_key: &str,
    f: F,
) -> Result<http::Response<Body>, HttpErr> {
    let claims = cookies
        .get(cookie_key)
        .as_ref()
        .map(|c| c.value())
        .or_else(|| {
            req.headers()
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|t| t.strip_prefix("Bearer "))
        })
        .and_then(|t| {
            let mut validation = Validation::new(Algorithm::RS256);
            validation.validate_aud = false;
            validation.leeway = 0;
            decode::<T>(t, jwt_rsa_de_pk, &validation).ok()
        })
        .map(|c| c.claims);

    let route = req.uri().to_string();

    if let Some(claims) = claims {
        let ext = req.extensions_mut();
        ext.insert(f(claims));
    } else if white_list.contains(&route.as_str()) {
        tracing::debug!("white list");
    } else {
        tracing::debug!("UNAUTHORIZED");
        return Err(HttpErr::ClientErr(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED".to_string(),
        ));
    }

    tracing::debug!("route: {}", route);

    let res = next.run(req).await;
    Ok(res)
}

pub fn route(state: AppState) -> Router {
    Router::new().merge(user::route(state))
}

pub mod err_key {
    pub const TMR: &str = "too_many_requests";
    pub const RTE: &str = "refresh_token_expction";
    pub const VCE: &str = "verfiy_code_expired";
    pub const VCW: &str = "verfiy_code_wrong";
    pub const XE: &str = "???";
    pub const EMS: &str = "email_send_error";
}
