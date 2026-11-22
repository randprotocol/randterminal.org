pub mod jobs;
use crate::{error::AppError, state::AppState, CIRCUITS_REV};
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// `ok`: this process answers. `sandbox`: the docker daemon answers (probe cached 30 s).
/// `chain_id`: the CHAINID a job gets when it names none. `constraint_set`: the zkVM constraint
/// set the vendored circuits prove under. The monorepo's site-sync check compares both with the
/// fleet, so a deploy left on an old chain or an old circuits pin shows up there.
pub async fn health(State(s): State<AppState>) -> Json<Value> {
    let sandbox = s.sandbox.available().await;
    Json(json!({
        "ok": true,
        "sandbox": sandbox,
        "sandbox_image": s.config.sandbox_image,
        "circuits_rev": CIRCUITS_REV,
        "chain_id": terminal_core::views::default_chain_id(),
        "constraint_set": rand_zkvm::machine::CONSTRAINT_SET
    }))
}

pub async fn not_found() -> AppError {
    AppError::NotFound("no such route".into())
}
pub mod runs;
pub mod samples;
