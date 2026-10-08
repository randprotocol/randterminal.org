use crate::{handlers, ratelimit, state::AppState};
use axum::{extract::DefaultBodyLimit, middleware, routing::get, Router};
use tower_http::{compression::CompressionLayer, cors::{Any, CorsLayer}, trace::TraceLayer};

/// Request bodies: 512 KB of source or a 2 MB ELF as base64 (about 2.7 MB), with room for JSON.
pub const BODY_LIMIT: usize = 4 << 20;

pub fn create_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(handlers::health))
        .fallback(handlers::not_found)
        .layer(middleware::from_fn_with_state(state.clone(), ratelimit::limit));
    Router::new()
        .nest("/api/v1", api)
        .fallback(handlers::not_found)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .with_state(state)
}
