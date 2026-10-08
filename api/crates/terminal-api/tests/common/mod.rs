#![allow(dead_code)]
pub mod fake;
use axum::{body::Body, http::{HeaderMap, Request, StatusCode}, Router};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

pub fn json_req(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let b = Request::builder().method(method).uri(uri).header("x-forwarded-for", "198.51.100.7");
    match body {
        Some(v) => b.header("content-type", "application/json").body(Body::from(v.to_string())).unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

pub async fn call(app: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())) };
    (status, headers, json)
}

pub fn test_app(runner: std::sync::Arc<dyn terminal_sandbox::Runner>) -> (Router, terminal_api::state::AppState, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let cfg = terminal_api::ApiConfig { data_dir: d.path().into(), samples_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../samples"), ..Default::default() };
    let state = terminal_api::state::AppState::new(cfg, runner);
    (terminal_api::routes::create_router(state.clone()), state, d)
}

pub async fn wait_job(app: &Router, id: &str) -> Value {
    for _ in 0..600 {
        let (_, _, v) = call(app, json_req("GET", &format!("/api/v1/jobs/{id}"), None)).await;
        if v["finished"] == true { return v; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("job {id} did not finish");
}

pub fn erc20_job() -> Value {
    serde_json::json!({"lang": "solidity", "files": [{"name": "ERC20.sol", "content": "contract ERC20 {}"}]})
}
