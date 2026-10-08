pub mod jobs;
use crate::{error::AppError, state::AppState, CIRCUITS_REV};
use axum::{extract::State, Json};
use serde_json::{json, Value};

pub async fn health(State(s): State<AppState>) -> Json<Value> {
    Json(
        json!({ "ok": true, "sandbox_image": s.config.sandbox_image, "circuits_rev": CIRCUITS_REV }),
    )
}

pub async fn not_found() -> AppError {
    AppError::NotFound("no such route".into())
}
pub mod runs;
pub mod samples;
