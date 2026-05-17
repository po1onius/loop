use crate::observability::{current_request_id, current_trace_id};
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::StatusCode;
use serde::Serialize;
use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    panic::Location,
};

const INTERNAL_ERROR_MESSAGE: &str = "server error";
const OPTION_MISSING_MESSAGE: &str = "required value is missing";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApiErrorCode {
    value: &'static str,
}

impl ApiErrorCode {
    pub const fn new(value: &'static str) -> Self {
        Self { value }
    }

    pub const fn as_str(self) -> &'static str {
        self.value
    }
}

impl Display for ApiErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.value)
    }
}

#[derive(Debug)]
pub enum ApiError {
    Client {
        status: StatusCode,
        code: ApiErrorCode,
        message: String,
    },
    Internal {
        code: ApiErrorCode,
        source: anyhow::Error,
        location: &'static Location<'static>,
    },
}

impl ApiError {
    pub fn client(status: StatusCode, code: impl Into<ApiErrorCode>) -> Self {
        let code = code.into();
        Self::client_msg(status, code, code.as_str())
    }

    pub fn client_msg(
        status: StatusCode,
        code: impl Into<ApiErrorCode>,
        message: impl Into<String>,
    ) -> Self {
        Self::Client {
            status,
            code: code.into(),
            message: message.into(),
        }
    }

    #[track_caller]
    pub fn internal(code: impl Into<ApiErrorCode>, source: impl Into<anyhow::Error>) -> Self {
        Self::Internal {
            code: code.into(),
            source: source.into(),
            location: Location::caller(),
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::Client { status, .. } => *status,
            Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn body(&self) -> ErrorBody {
        match self {
            Self::Client { code, message, .. } => ErrorBody {
                code: code.as_str().to_string(),
                message: message.clone(),
                trace_id: current_trace_id(),
            },
            Self::Internal { code, .. } => ErrorBody {
                code: code.as_str().to_string(),
                message: INTERNAL_ERROR_MESSAGE.to_string(),
                trace_id: current_trace_id(),
            },
        }
    }

    fn log(&self, status: StatusCode) {
        let request_id = current_request_id().unwrap_or_default();
        let trace_id = current_trace_id().unwrap_or_default();
        match self {
            Self::Client { code, message, .. } => {
                if matches!(
                    status,
                    StatusCode::UNAUTHORIZED
                        | StatusCode::FORBIDDEN
                        | StatusCode::TOO_MANY_REQUESTS
                ) {
                    tracing::warn!(
                        event = "http.error_response",
                        error_kind = "client",
                        error_code = %code,
                        error_message = %message,
                        http_status = status.as_u16(),
                        request_id = %request_id,
                        trace_id = %trace_id,
                        "HTTP client error response {} {}",
                        status.as_u16(),
                        code,
                    );
                } else {
                    tracing::info!(
                        event = "http.error_response",
                        error_kind = "client",
                        error_code = %code,
                        error_message = %message,
                        http_status = status.as_u16(),
                        request_id = %request_id,
                        trace_id = %trace_id,
                        "HTTP client error response {} {}",
                        status.as_u16(),
                        code,
                    );
                }
            }
            Self::Internal {
                code,
                source,
                location,
            } => {
                let chain = source
                    .chain()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(": ");
                tracing::error!(
                    event = "http.error_response",
                    error_kind = "internal",
                    error_code = %code,
                    error_location = %location,
                    error_chain = %chain,
                    error_source = ?source,
                    http_status = status.as_u16(),
                    request_id = %request_id,
                    trace_id = %trace_id,
                    "HTTP internal error response {} {}",
                    status.as_u16(),
                    code,
                );
            }
        }
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client { code, message, .. } => write!(f, "{code}: {message}"),
            Self::Internal { code, .. } => write!(f, "{code}: {INTERNAL_ERROR_MESSAGE}"),
        }
    }
}

impl StdError for ApiError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Client { .. } => None,
            Self::Internal { source, .. } => Some(source.as_ref()),
        }
    }
}

#[cfg(feature = "db")]
impl From<diesel::result::Error> for ApiError {
    #[track_caller]
    fn from(err: diesel::result::Error) -> Self {
        Self::internal(ApiErrorCode::new("db_error"), err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        self.log(status);
        (status, Json(self.body())).into_response()
    }
}

pub trait ResultExt<T> {
    fn internal(self, code: impl Into<ApiErrorCode>) -> Result<T, ApiError>;
    fn client(self, status: StatusCode, code: impl Into<ApiErrorCode>) -> Result<T, ApiError>;
    fn client_msg(
        self,
        status: StatusCode,
        code: impl Into<ApiErrorCode>,
        message: impl Into<String>,
    ) -> Result<T, ApiError>;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    #[track_caller]
    fn internal(self, code: impl Into<ApiErrorCode>) -> Result<T, ApiError> {
        let code = code.into();
        self.map_err(|err| ApiError::internal(code, err))
    }

    #[track_caller]
    fn client(self, status: StatusCode, code: impl Into<ApiErrorCode>) -> Result<T, ApiError> {
        let code = code.into();
        self.client_msg(status, code, code.as_str())
    }

    #[track_caller]
    fn client_msg(
        self,
        status: StatusCode,
        code: impl Into<ApiErrorCode>,
        message: impl Into<String>,
    ) -> Result<T, ApiError> {
        let code = code.into();
        let message = message.into();
        self.map_err(|_| ApiError::client_msg(status, code, message))
    }
}

pub trait OptionExt<T> {
    fn internal(self, code: impl Into<ApiErrorCode>) -> Result<T, ApiError>;
    fn client(self, status: StatusCode, code: impl Into<ApiErrorCode>) -> Result<T, ApiError>;
    fn client_msg(
        self,
        status: StatusCode,
        code: impl Into<ApiErrorCode>,
        message: impl Into<String>,
    ) -> Result<T, ApiError>;
}

impl<T> OptionExt<T> for Option<T> {
    #[track_caller]
    fn internal(self, code: impl Into<ApiErrorCode>) -> Result<T, ApiError> {
        let code = code.into();
        self.ok_or_else(|| ApiError::internal(code, anyhow::anyhow!(OPTION_MISSING_MESSAGE)))
    }

    #[track_caller]
    fn client(self, status: StatusCode, code: impl Into<ApiErrorCode>) -> Result<T, ApiError> {
        let code = code.into();
        self.client_msg(status, code, code.as_str())
    }

    #[track_caller]
    fn client_msg(
        self,
        status: StatusCode,
        code: impl Into<ApiErrorCode>,
        message: impl Into<String>,
    ) -> Result<T, ApiError> {
        let code = code.into();
        let message = message.into();
        self.ok_or_else(|| ApiError::client_msg(status, code, message))
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_error_uses_configured_status() {
        let response =
            ApiError::client(StatusCode::BAD_REQUEST, ApiErrorCode::new("invalid_input"))
                .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn internal_error_uses_internal_server_error_status() {
        let response = ApiError::internal(
            ApiErrorCode::new("db_error"),
            anyhow::anyhow!("connection failed"),
        )
        .into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn option_client_maps_none_to_client_error() {
        let err = Option::<()>::None
            .client(StatusCode::UNAUTHORIZED, ApiErrorCode::new("unauthorized"))
            .expect_err("none should become a client error");

        match err {
            ApiError::Client { status, code, .. } => {
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert_eq!(code.as_str(), "unauthorized");
            }
            ApiError::Internal { .. } => panic!("expected client error"),
        }
    }
}
