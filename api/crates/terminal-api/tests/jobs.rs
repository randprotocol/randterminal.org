mod common;
use axum::http::StatusCode;
use common::{fake::*, *};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn submit_poll_and_read_every_view() {
    let (app, _s, _d) = test_app(fake_runner());
    let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let job = wait_job(&app, &id).await;
    assert_eq!(job["build"]["status"], "done", "{job}");
    let (st, _, c) = call(
        &app,
        json_req("GET", &format!("/api/v1/jobs/{id}/compile"), None),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(c["lang"], "evm");
    assert_eq!(c["blocks"], 74);
    let (_, _, t) = call(
        &app,
        json_req("GET", &format!("/api/v1/jobs/{id}/transpile"), None),
    )
    .await;
    assert!(t["op_map"].as_array().unwrap().len() > 700);
    let (_, _, i) = call(
        &app,
        json_req("GET", &format!("/api/v1/jobs/{id}/image"), None),
    )
    .await;
    assert_eq!(i["words"], 11_686);
    let res = app
        .clone()
        .oneshot(json_req(
            "GET",
            &format!("/api/v1/jobs/{id}/files/image.bin"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.headers()["content-type"], "application/octet-stream");
    // erc20.image.bin is 46_244 bytes (RULING R7; the brief said 46_760).
    assert_eq!(
        res.into_body().collect().await.unwrap().to_bytes().len(),
        46_244
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn views_are_409_until_ready_and_unknown_jobs_404() {
    let (app, _s, _d) = test_app(slow_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap();
    let (st, _, e) = call(
        &app,
        json_req("GET", &format!("/api/v1/jobs/{id}/image"), None),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    let (st, _, _) = call(
        &app,
        json_req("GET", &format!("/api/v1/jobs/{}", "0".repeat(32)), None),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _, _) = call(&app, json_req("GET", "/api/v1/jobs/..%2F..%2Fetc", None)).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn events_stream_stage_changes_and_ends_with_done() {
    let (app, _s, _d) = test_app(slow_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap();
    let res = app
        .clone()
        .oneshot(json_req("GET", &format!("/api/v1/jobs/{id}/events"), None))
        .await
        .unwrap();
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    let body = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        res.into_body().collect(),
    )
    .await
    .unwrap()
    .unwrap()
    .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(text.starts_with("event: job\n"), "{text}");
    assert!(text.contains("event: stage\n"));
    assert!(text.contains("\"status\":\"done\""));
    assert!(text.contains("event: console\n"), "{text}");
    assert!(text.contains("\"type\":\"stage\""), "{text}");
    assert!(text.trim_end().ends_with("event: done\ndata: {}"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn events_on_a_finished_job_are_snapshot_then_done() {
    let (app, _s, _d) = test_app(fake_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap();
    wait_job(&app, id).await;
    let res = app
        .clone()
        .oneshot(json_req("GET", &format!("/api/v1/jobs/{id}/events"), None))
        .await
        .unwrap();
    let text =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).into_owned();
    assert_eq!(text.matches("event: ").count(), 2, "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_jobs_and_the_job_rate_limit() {
    let (app, _s, _d) = test_app(fake_runner());
    let (st, _, v) = call(
        &app,
        json_req(
            "POST",
            "/api/v1/jobs",
            Some(serde_json::json!({"lang": "solidity", "files": []})),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    let (st, _, _) = call(
        &app,
        json_req(
            "POST",
            "/api/v1/jobs",
            Some(serde_json::json!({"lang": "cobol", "files": []})),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let huge = serde_json::json!({"lang": "solidity", "files": [{"name": "A.sol", "content": "x".repeat(5 << 20)}]});
    let (st, _, _) = call(&app, json_req("POST", "/api/v1/jobs", Some(huge))).await;
    assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
    // jobs_rpm = 10 and the limiter counts every POST /jobs before the body is read, so the three
    // refusals above already used 3 of the 10. Submissions 4..=10 (7 of them) are accepted and the
    // 8th loop iteration, the 11th request overall, is the first to be refused.
    for n in 1..=7 {
        let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
        assert_eq!(st, StatusCode::CREATED, "submission {n}: {v}");
    }
    let (st, _, _) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test(flavor = "multi_thread")]
async fn events_on_an_unknown_job_404_and_are_never_gzipped() {
    let (app, _s, _d) = test_app(fake_runner());
    let (st, _, _) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/jobs/{}/events", "0".repeat(32)),
            None,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap();
    let mut req = json_req("GET", &format!("/api/v1/jobs/{id}/events"), None);
    req.headers_mut()
        .insert("accept-encoding", "gzip".parse().unwrap());
    let res = app.clone().oneshot(req).await.unwrap();
    assert!(
        res.headers().get("content-encoding").is_none(),
        "{:?}",
        res.headers()
    );
}

fn panicking_runner() -> std::sync::Arc<dyn terminal_sandbox::Runner> {
    std::sync::Arc::new(terminal_sandbox::FakeRunner::new(
        |_: &terminal_sandbox::RunSpec| -> terminal_sandbox::RunOutput {
            panic!("runner exploded")
        },
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_panicking_pipeline_still_finishes_the_job_and_ends_the_stream() {
    let (app, _s, _d) = test_app(panicking_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap();
    let res = app
        .clone()
        .oneshot(json_req("GET", &format!("/api/v1/jobs/{id}/events"), None))
        .await
        .unwrap();
    let body = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        res.into_body().collect(),
    )
    .await
    .unwrap()
    .unwrap()
    .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(text.trim_end().ends_with("event: done\ndata: {}"), "{text}");
    let job = wait_job(&app, id).await;
    assert_eq!(job["compile"]["status"], "failed", "{job}");
    assert_eq!(job["compile"]["error"]["kind"], "internal");
    assert_eq!(
        job["compile"]["error"]["message"],
        "the pipeline stopped unexpectedly"
    );
    assert_eq!(job["transpile"]["status"], "queued");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_left_unfinished_by_a_restart_is_failed_and_streams_snapshot_then_done() {
    let (app, state, d) = test_app(slow_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap().to_string();
    // Freeze the on-disk state of an in-flight job (compile running), as a killed process leaves it.
    let path = state.store.dir(&id).unwrap().join("job.json");
    wait_job(&app, &id).await;
    let mut j: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    j["finished"] = false.into();
    j["compile"] = serde_json::json!({"status": "running", "started": 1});
    j["transpile"] = serde_json::json!({"status": "queued"});
    j["build"] = serde_json::json!({"status": "queued"});
    std::fs::write(&path, j.to_string()).unwrap();
    // A second process over the same data directory.
    let cfg = terminal_api::ApiConfig {
        data_dir: d.path().into(),
        ..Default::default()
    };
    let state2 = terminal_api::state::AppState::new(cfg, fake_runner());
    let app2 = terminal_api::routes::create_router(state2);
    let job = wait_job(&app2, &id).await;
    assert_eq!(job["compile"]["status"], "failed", "{job}");
    assert_eq!(
        job["compile"]["error"]["message"],
        "interrupted by a restart"
    );
    assert_eq!(job["compile"]["error"]["kind"], "internal");
    let res = app2
        .clone()
        .oneshot(json_req("GET", &format!("/api/v1/jobs/{id}/events"), None))
        .await
        .unwrap();
    let text =
        String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).into_owned();
    assert_eq!(text.matches("event: ").count(), 2, "{text}");
    assert!(
        text.starts_with("event: job\n") && text.trim_end().ends_with("event: done\ndata: {}"),
        "{text}"
    );
}
