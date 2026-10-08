mod common;
use axum::http::StatusCode;
use common::*;
use terminal_api::{
    error::AppError,
    ratelimit::{Decision, RateLimiter},
    routes::create_router,
    state::AppState,
    ApiConfig,
};

fn app(cfg: ApiConfig) -> axum::Router {
    // Each router gets its own data directory, kept alive for the test binary's lifetime.
    let d: &'static tempfile::TempDir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    create_router(AppState::for_tests(ApiConfig {
        data_dir: d.path().into(),
        ..cfg
    }))
}

#[tokio::test]
async fn health_names_the_sandbox_and_circuits() {
    let cfg = ApiConfig {
        sandbox_image: "terminal-sandbox:9bf3844".into(),
        ..ApiConfig::default()
    };
    let (s, _, v) = call(&app(cfg), json_req("GET", "/api/v1/health", None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["ok"], true);
    assert_eq!(v["sandbox_image"], "terminal-sandbox:9bf3844");
    let rev = v["circuits_rev"].as_str().unwrap();
    assert!(
        rev.len() == 7
            && rev
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "rev {rev}"
    );
}

#[tokio::test]
async fn unknown_routes_are_json_404s() {
    let (s, _, v) = call(
        &app(ApiConfig::default()),
        json_req("GET", "/api/v1/nope", None),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(v["error"], "not_found");
}

#[tokio::test]
async fn the_api_bucket_limits() {
    let cfg = ApiConfig {
        api_rpm: 2,
        trust_proxy: true,
        ..ApiConfig::default()
    };
    let a = app(cfg);
    for _ in 0..2 {
        assert_eq!(
            call(&a, json_req("GET", "/api/v1/health", None)).await.0,
            StatusCode::OK
        );
    }
    let (s, h, v) = call(&a, json_req("GET", "/api/v1/health", None)).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(h.get("retry-after").is_some());
    assert_eq!(v["error"], "rate_limited");
}

#[tokio::test]
async fn error_shapes() {
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;
    for (e, code, tag) in [
        (AppError::NotFound("x".into()), 404, "not_found"),
        (AppError::BadRequest("x".into()), 400, "bad_request"),
        (
            AppError::PayloadTooLarge("x".into()),
            413,
            "payload_too_large",
        ),
        (AppError::Conflict("x".into()), 409, "conflict"),
        (
            AppError::TooManyRequests { retry_after: 3 },
            429,
            "rate_limited",
        ),
        (AppError::Unavailable("x".into()), 503, "unavailable"),
        (AppError::Internal("secret".into()), 500, "internal"),
    ] {
        let r = e.into_response();
        assert_eq!(r.status().as_u16(), code, "{tag}");
        let bytes = r.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"], tag);
        if tag == "internal" {
            assert_eq!(v["message"], "internal error");
            assert!(!String::from_utf8_lossy(&bytes).contains("secret"));
        }
    }
}

fn req_from(method: &str, uri: &str, ip: &str) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", ip)
        .body(axum::body::Body::empty())
        .unwrap()
}

#[tokio::test]
async fn the_jobs_bucket_limits_and_is_independent() {
    let a = app(ApiConfig {
        jobs_rpm: 1,
        trust_proxy: true,
        ..ApiConfig::default()
    });
    assert_ne!(
        call(&a, req_from("POST", "/api/v1/jobs", "10.0.0.1"))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let (s, h, v) = call(&a, req_from("POST", "/api/v1/jobs", "10.0.0.1")).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(h.get("retry-after").is_some());
    assert_eq!(v["error"], "rate_limited");
    // another client is unaffected
    assert_ne!(
        call(&a, req_from("POST", "/api/v1/jobs", "10.0.0.2"))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // the api bucket is separate
    assert_eq!(
        call(&a, req_from("GET", "/api/v1/health", "10.0.0.1"))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_runs_bucket_limits() {
    let a = app(ApiConfig {
        runs_rpm: 1,
        trust_proxy: true,
        ..ApiConfig::default()
    });
    assert_ne!(
        call(&a, req_from("POST", "/api/v1/jobs/abc/runs", "10.0.0.1"))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call(&a, req_from("POST", "/api/v1/jobs/abc/runs", "10.0.0.1"))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_ne!(
        call(&a, req_from("POST", "/api/v1/jobs/abc/runs", "10.0.0.2"))
            .await
            .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        call(&a, req_from("GET", "/api/v1/health", "10.0.0.1"))
            .await
            .0,
        StatusCode::OK
    );
}

#[test]
fn client_ip_takes_the_last_forwarded_hop() {
    let mut h = axum::http::HeaderMap::new();
    h.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
    assert_eq!(
        terminal_api::ratelimit::client_ip(&h, None, true),
        "5.6.7.8"
    );
    assert_eq!(
        terminal_api::ratelimit::client_ip(&h, None, false),
        "unknown"
    );
}

#[test]
fn limiter_windows() {
    let l = RateLimiter::new();
    assert_eq!(l.check("a", 1, 100), Decision::Allowed { remaining: 0 });
    assert_eq!(l.check("a", 1, 110), Decision::Limited { retry_after: 10 });
    assert_eq!(l.check("a", 1, 120), Decision::Allowed { remaining: 0 });
    assert_eq!(
        l.check("a", 0, 120),
        Decision::Allowed {
            remaining: u32::MAX
        }
    );
}
