use crate::{
    error::{ApiResult, AppError},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use terminal_core::views::{SampleMeta, SampleView};

pub async fn list(State(s): State<AppState>) -> Json<Vec<SampleMeta>> {
    Json(s.samples.iter().map(|x| x.meta.clone()).collect())
}

pub async fn get(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<SampleView>> {
    s.samples
        .iter()
        .find(|x| x.meta.id == id)
        .cloned()
        .map(Json)
        .ok_or_else(|| AppError::NotFound(format!("sample {id}")))
}
