use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AmaError {
    #[error("malformed request: {message}")]
    BadRequest { message: String },

    #[error("impossible")]
    Impossible,

    #[error("validation error: {message}")]
    Validation { error_class: String, message: String },

    #[error("conflict: {message}")]
    Conflict { message: String },

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("unsupported media type")]
    UnsupportedMediaType,

    #[error("rate limit exceeded")]
    RateLimited,

    #[error("service unavailable: {message}")]
    ServiceUnavailable { message: String },
}

impl IntoResponse for AmaError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            AmaError::Impossible => (
                StatusCode::FORBIDDEN,
                json!({"status": "impossible"}),
            ),
            AmaError::BadRequest { message } => (
                StatusCode::BAD_REQUEST,
                json!({"status": "error", "error_class": "bad_request", "message": message}),
            ),
            AmaError::Validation { error_class, message } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({"status": "error", "error_class": error_class, "message": message}),
            ),
            AmaError::Conflict { message } => (
                StatusCode::CONFLICT,
                json!({"status": "error", "error_class": "conflict", "message": message}),
            ),
            AmaError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"status": "error", "error_class": "payload_too_large", "message": "payload exceeds limit"}),
            ),
            AmaError::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                json!({"status": "error", "error_class": "unsupported_media_type", "message": "expected application/json"}),
            ),
            AmaError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                json!({"status": "error", "error_class": "rate_limited", "message": "rate limit exceeded"}),
            ),
            AmaError::ServiceUnavailable { message } => (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"status": "error", "error_class": "service_unavailable", "message": message}),
            ),
        };
        (status, axum::Json(body)).into_response()
    }
}
