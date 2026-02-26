use std::time::Duration;

use axum::response::IntoResponse;
use http::StatusCode;
use jsonwebtoken::{DecodingKey, EncodingKey};

pub mod account;

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    jwt_enc: EncodingKey,
    jwt_dec: DecodingKey,
    jwt_issuer: String,
    access_ttl: Duration,
    refresh_ttl: Duration,
}

#[derive(Error, Debug)]
enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("internal error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (code, msg) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (code, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}
