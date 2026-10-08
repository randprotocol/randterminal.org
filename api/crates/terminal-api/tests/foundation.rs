mod common;
use axum::http::StatusCode;
use common::*;
use terminal_api::{error::AppError, ratelimit::{Decision, RateLimiter}, routes::create_router, state::AppState, ApiConfig};

fn app(cfg: ApiConfig) -> axum::Router {
    // Each router gets its own data directory, kept alive for the test binary's lifetime.
    let d: &'static tempfile::TempDir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    create_router(AppState::for_tests(ApiConfig { data_dir: d.path().into(), ..cfg }))
}

#[tokio::test]
async fn health_names_the_sandbox_and_circuits() {
    let cfg = ApiConfig { sandbox_image: "terminal-sandbox:9bf3844".into(), ..ApiConfig::default() };
    let (s, _, v) = call(&app(cfg), json_req("GET", "/api/v1/health", None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["ok"], true);
    assert_eq!(v["sandbox_image"], "terminal-sandbox:9bf3844");
    assert_eq!(v["circuits_rev"].as_str().unwrap().len(), 7);
}

#[tokio::test]
async fn unknown_routes_are_json_404s() {
    let (s, _, v) = call(&app(ApiConfig::default()), json_req("GET", "/api/v1/nope", None)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(v["error"], "not_found");
}

#[tokio::test]
async fn the_api_bucket_limits() {
    let cfg = ApiConfig { api_rpm: 2, trust_proxy: true, ..ApiConfig::default() };
    let a = app(cfg);
    for _ in 0..2 { assert_eq!(call(&a, json_req("GET", "/api/v1/health", None)).await.0, StatusCode::OK); }
    let (s, h, v) = call(&a, json_req("GET", "/api/v1/health", None)).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(h.get("retry-after").is_some());
    assert_eq!(v["error"], "rate_limited");
}

#[test]
fn error_shapes() {
    use axum::response::IntoResponse;
    for (e, code, tag) in [
        (AppError::BadRequest("x".into()), 400, "bad_request"),
        (AppError::PayloadTooLarge("x".into()), 413, "payload_too_large"),
        (AppError::Conflict("x".into()), 409, "conflict"),
        (AppError::Unavailable("x".into()), 503, "unavailable"),
        (AppError::Internal("secret".into()), 500, "internal"),
    ] {
        let r = e.into_response();
        assert_eq!(r.status().as_u16(), code, "{tag}");
    }
}

#[test]
fn limiter_windows() {
    let l = RateLimiter::new();
    assert_eq!(l.check("a", 1, 100), Decision::Allowed { remaining: 0 });
    assert_eq!(l.check("a", 1, 110), Decision::Limited { retry_after: 10 });
    assert_eq!(l.check("a", 1, 120), Decision::Allowed { remaining: 0 });
    assert_eq!(l.check("a", 0, 120), Decision::Allowed { remaining: u32::MAX });
}
