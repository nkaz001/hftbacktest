//! REST API handlers

pub mod bots;
pub mod config;
pub mod data;
pub mod logs;
pub mod status;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// API error response
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

impl ApiError {
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal_error", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new("bad_request", message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new("unauthorized", message)
    }

    pub fn supervisor_unavailable() -> Self {
        Self::new("supervisor_unavailable", "Supervisor is not running")
    }
}

/// API result type
pub type ApiResult<T> = Result<Json<T>, ApiErrorResponse>;

/// Wrapper for API error responses with status codes
pub struct ApiErrorResponse {
    status: StatusCode,
    error: ApiError,
}

impl ApiErrorResponse {
    pub fn new(status: StatusCode, error: ApiError) -> Self {
        Self { status, error }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, ApiError::internal(message))
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, ApiError::not_found(message))
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiError::bad_request(message))
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, ApiError::unauthorized(message))
    }

    pub fn service_unavailable(error: ApiError) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, error)
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        (self.status, Json(self.error)).into_response()
    }
}

impl From<anyhow::Error> for ApiErrorResponse {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(error = %err, "API error");
        Self::internal(err.to_string())
    }
}

/// Success response wrapper
#[derive(Debug, Serialize)]
pub struct ApiSuccess<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> ApiSuccess<T> {
    pub fn new(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn with_message(message: impl Into<String>) -> ApiSuccess<()> {
        ApiSuccess {
            success: true,
            data: None,
            message: Some(message.into()),
        }
    }
}
