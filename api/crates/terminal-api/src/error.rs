use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("not found: {0}")] NotFound(String),
    #[error("bad request: {0}")] BadRequest(String),
    #[error("too large: {0}")] PayloadTooLarge(String),
    #[error("conflict: {0}")] Conflict(String),
    #[error("rate limited")] TooManyRequests { retry_after: u64 },
    #[error("unavailable: {0}")] Unavailable(String),
    #[error("internal: {0}")] Internal(String),
}

impl From<std::io::Error> for AppError { fn from(e: std::io::Error) -> Self { AppError::Internal(e.to_string()) } }
impl From<serde_json::Error> for AppError { fn from(e: serde_json::Error) -> Self { AppError::Internal(e.to_string()) } }

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, tag, message) = match self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            AppError::PayloadTooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            AppError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", m),
            AppError::TooManyRequests { retry_after } => {
                let body = Json(json!({ "error": "rate_limited", "message": format!("too many requests; retry in {retry_after} seconds") }));
                return (StatusCode::TOO_MANY_REQUESTS, [("retry-after", retry_after.to_string())], body).into_response();
            }
            AppError::Internal(m) => { tracing::error!("internal error: {m}"); (StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error".to_string()) }
        };
        (status, Json(json!({ "error": tag, "message": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, AppError>;
