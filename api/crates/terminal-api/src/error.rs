use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("too large: {0}")]
    PayloadTooLarge(String),
    #[error("conflict: {0}")]
    Conflict(String),
    /// One client has too many unfinished jobs (429, unlike the rate limit it has no retry time).
    #[error("too many jobs: {0}")]
    TooManyJobs(String),
    #[error("rate limited")]
    TooManyRequests { retry_after: u64 },
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, tag, message) = match self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            AppError::PayloadTooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large", m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            AppError::TooManyJobs(m) => (StatusCode::TOO_MANY_REQUESTS, "too_many_jobs", m),
            AppError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", m),
            AppError::TooManyRequests { retry_after } => {
                let body = Json(
                    json!({ "error": "rate_limited", "message": format!("too many requests; retry in {retry_after} seconds") }),
                );
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", retry_after.to_string())],
                    body,
                )
                    .into_response();
            }
            AppError::Internal(m) => {
                tracing::error!("internal error: {m}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": tag, "message": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, AppError>;

/// Gives axum's own extractor rejections (malformed JSON, a bad query string, a body over the
/// limit, a missing content type) the api's `{error, message}` shape; they are plain text by
/// default. Responses that are already JSON pass through untouched.
pub async fn json_rejections(res: Response) -> Response {
    let status = res.status();
    let tag = match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_media_type",
        StatusCode::UNPROCESSABLE_ENTITY => "unprocessable",
        _ => return res,
    };
    let json = res
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if json {
        return res;
    }
    let text = axum::body::to_bytes(res.into_body(), 64 << 10)
        .await
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .unwrap_or_default();
    let message = if text.is_empty() {
        status.canonical_reason().unwrap_or("error").to_lowercase()
    } else {
        text
    };
    (status, Json(json!({ "error": tag, "message": message }))).into_response()
}
