use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::StatusCode;
use opentelemetry::trace::TraceContextExt;
use serde::Serialize;
use std::{
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    panic::Location,
};
use tracing_opentelemetry::OpenTelemetrySpanExt;

const INTERNAL_ERROR_MESSAGE: &str = "server error";
const OPTION_MISSING_MESSAGE: &str = "required value is missing";

#[derive(Debug)]
pub enum ApiError {
    Client {
        status: StatusCode,
        code: &'static str,
        message: String,
    },
    Internal {
        code: &'static str,
        source: anyhow::Error,
        location: &'static Location<'static>,
    },
}

impl ApiError {
    pub fn client(status: StatusCode, code: &'static str) -> Self {
        Self::client_msg(status, code, code)
    }

    pub fn client_msg(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self::Client {
            status,
            code,
            message: message.into(),
        }
    }

    #[track_caller]
    pub fn internal(code: &'static str, source: impl Into<anyhow::Error>) -> Self {
        Self::Internal {
            code,
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
                code: (*code).to_string(),
                message: message.clone(),
                trace_id: current_trace_id(),
            },
            Self::Internal { code, .. } => ErrorBody {
                code: (*code).to_string(),
                message: INTERNAL_ERROR_MESSAGE.to_string(),
                trace_id: current_trace_id(),
            },
        }
    }

    fn log(&self, status: StatusCode) {
        match self {
            Self::Client { code, .. } => {
                tracing::debug!(
                    error.code = %code,
                    http.status_code = status.as_u16(),
                    "client error response"
                );
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
                    error.code = %code,
                    error.location = %location,
                    error.chain = %chain,
                    error.source = ?source,
                    http.status_code = status.as_u16(),
                    "internal error response"
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

impl From<diesel::result::Error> for ApiError {
    #[track_caller]
    fn from(err: diesel::result::Error) -> Self {
        Self::internal("db_error", err)
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
    fn internal(self, code: &'static str) -> Result<T, ApiError>;
    fn client(self, status: StatusCode, code: &'static str) -> Result<T, ApiError>;
    fn client_msg(
        self,
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
    ) -> Result<T, ApiError>;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    #[track_caller]
    fn internal(self, code: &'static str) -> Result<T, ApiError> {
        self.map_err(|err| ApiError::internal(code, err))
    }

    #[track_caller]
    fn client(self, status: StatusCode, code: &'static str) -> Result<T, ApiError> {
        self.client_msg(status, code, code)
    }

    #[track_caller]
    fn client_msg(
        self,
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
    ) -> Result<T, ApiError> {
        self.map_err(|_| ApiError::client_msg(status, code, message))
    }
}

pub trait OptionExt<T> {
    fn internal(self, code: &'static str) -> Result<T, ApiError>;
    fn client(self, status: StatusCode, code: &'static str) -> Result<T, ApiError>;
    fn client_msg(
        self,
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
    ) -> Result<T, ApiError>;
}

impl<T> OptionExt<T> for Option<T> {
    #[track_caller]
    fn internal(self, code: &'static str) -> Result<T, ApiError> {
        self.ok_or_else(|| ApiError::internal(code, anyhow::anyhow!(OPTION_MISSING_MESSAGE)))
    }

    #[track_caller]
    fn client(self, status: StatusCode, code: &'static str) -> Result<T, ApiError> {
        self.client_msg(status, code, code)
    }

    #[track_caller]
    fn client_msg(
        self,
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
    ) -> Result<T, ApiError> {
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

fn current_trace_id() -> Option<String> {
    let context = tracing::Span::current().context();
    let span_context = context.span().span_context().clone();
    span_context
        .is_valid()
        .then(|| span_context.trace_id().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_error_uses_configured_status() {
        let response = ApiError::client(StatusCode::BAD_REQUEST, "invalid_input").into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn internal_error_uses_internal_server_error_status() {
        let response =
            ApiError::internal("db_error", anyhow::anyhow!("connection failed")).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn option_client_maps_none_to_client_error() {
        let err = Option::<()>::None
            .client(StatusCode::UNAUTHORIZED, "unauthorized")
            .expect_err("none should become a client error");

        match err {
            ApiError::Client { status, code, .. } => {
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert_eq!(code, "unauthorized");
            }
            ApiError::Internal { .. } => panic!("expected client error"),
        }
    }
}
