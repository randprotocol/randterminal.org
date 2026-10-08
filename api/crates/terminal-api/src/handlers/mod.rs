pub mod jobs;
use crate::{error::AppError, state::AppState, CIRCUITS_REV};
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// `ok`: this process answers. `sandbox`: the docker daemon answers (probe cached 30 s).
pub async fn health(State(s): State<AppState>) -> Json<Value> {
    let sandbox = s.sandbox.available().await;
    Json(json!({
        "ok": true,
        "sandbox": sandbox,
        "sandbox_image": s.config.sandbox_image,
        "circuits_rev": CIRCUITS_REV
    }))
}

pub async fn not_found() -> AppError {
    AppError::NotFound("no such route".into())
}
pub mod runs;
pub mod samples;
