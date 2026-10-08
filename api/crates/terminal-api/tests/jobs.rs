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
        // One client may only have a few unfinished jobs; this test is about the rate limit.
        wait_job(&app, v["id"].as_str().unwrap()).await;
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

fn job_from(ip: &str) -> axum::http::Request<axum::body::Body> {
    let mut r = json_req("POST", "/api/v1/jobs", Some(erc20_job()));
    r.headers_mut()
        .insert("x-forwarded-for", ip.parse().unwrap());
    r
}

/// I5: one client may have only a few unfinished jobs (429), and the queue as a whole is
/// bounded (503); both say why.
#[tokio::test(flavor = "multi_thread")]
async fn unfinished_jobs_are_bounded_per_client_and_in_total() {
    let gated = GatedRunner::new();
    let (app, _s, _d) = test_app_with(gated.clone(), |c| {
        c.trust_proxy = true;
        c.max_active_jobs_per_ip = 2;
        c.max_queued_jobs = 3;
    });
    for _ in 0..2 {
        assert_eq!(
            call(&app, job_from("10.0.0.1")).await.0,
            StatusCode::CREATED
        );
    }
    let (st, _, v) = call(&app, job_from("10.0.0.1")).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "{v}");
    assert_eq!(v["error"], "too_many_jobs");
    assert!(
        v["message"].as_str().unwrap().contains("2 unfinished"),
        "{v}"
    );
    assert_eq!(
        call(&app, job_from("10.0.0.2")).await.0,
        StatusCode::CREATED
    );
    let (st, _, v) = call(&app, job_from("10.0.0.3")).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert!(
        v["message"].as_str().unwrap().contains("queue is full"),
        "{v}"
    );
    gated.open();
    let (st, _, v) = loop {
        let r = call(&app, job_from("10.0.0.1")).await;
        if r.0 == StatusCode::CREATED {
            break r;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    assert_eq!(st, StatusCode::CREATED, "{v}");
}

/// Size in bytes of one finished erc20 job under the fake sandbox.
async fn one_job_size() -> u64 {
    let (app, s, _d) = test_app(fake_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap().to_string();
    wait_job(&app, &id).await;
    terminal_api::jobs::dir_size(&s.store.dir(&id).unwrap())
}

/// An app whose cap holds three finished jobs and a half; four finished jobs put it over.
async fn app_over_cap() -> (
    axum::Router,
    terminal_api::state::AppState,
    tempfile::TempDir,
    u64,
    Vec<String>,
) {
    let cap = one_job_size().await * 7 / 2;
    let (app, s, d) = test_app_with(fake_runner(), |c| c.data_cap_bytes = cap);
    let mut ids = vec![];
    for _ in 0..4 {
        let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
        assert_eq!(st, StatusCode::CREATED, "{v}");
        let id = v["id"].as_str().unwrap().to_string();
        wait_job(&app, &id).await;
        ids.push(id);
    }
    (app, s, d, cap, ids)
}

fn old_bytes(s: &terminal_api::state::AppState, ids: &[String]) -> u64 {
    ids.iter()
        .filter_map(|i| s.store.dir(i).ok())
        .map(|d| terminal_api::jobs::dir_size(&d))
        .sum()
}

/// At the cap with evictable finished jobs, a new job is accepted at once: the request sweeps
/// synchronously down to 90% of the cap instead of answering 503.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_job_at_the_cap_sweeps_synchronously_and_is_accepted() {
    let (app, s, _d, cap, ids) = app_over_cap().await;
    let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let new = v["id"].as_str().unwrap().to_string();
    assert!(
        old_bytes(&s, &ids) <= cap / 10 * 9,
        "evicted to the low-water mark"
    );
    assert!(old_bytes(&s, &ids) > 0, "not everything was evicted");
    wait_job(&app, &new).await;
}

/// Only running jobs fill the cap: nothing can be evicted, so the new job gets 503 and the
/// running one is untouched.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_job_gets_503_when_only_running_jobs_fill_the_cap() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let gate = std::sync::Arc::new(AtomicBool::new(false));
    let g = gate.clone();
    let runner: std::sync::Arc<dyn terminal_sandbox::Runner> = std::sync::Arc::new(
        terminal_sandbox::FakeRunner::new(move |spec: &terminal_sandbox::RunSpec| {
            while !g.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            fake_answer(spec)
        }),
    );
    let (app, s, _d) = test_app_with(runner, |c| c.data_cap_bytes = 1);
    let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let running = v["id"].as_str().unwrap().to_string();
    s.store.sweep(terminal_api::ratelimit::unix_now()); // measures the running job's directory
    let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert!(v["message"].as_str().unwrap().contains("storage"), "{v}");
    assert!(
        s.store.dir(&running).is_ok(),
        "a running job is never evicted"
    );
    gate.store(true, Ordering::Release);
    wait_job(&app, &running).await;
}

/// Two creates at the cap at the same moment: one sweeps, the other waits for it; both succeed.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_creates_at_the_cap_both_succeed_after_one_sweep() {
    let (app, _s, _d, _cap, _ids) = app_over_cap().await;
    let post = || call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job())));
    let ((a, _, va), (b, _, vb)) = tokio::join!(post(), post());
    assert_eq!(
        (a, b),
        (StatusCode::CREATED, StatusCode::CREATED),
        "{va} {vb}"
    );
}
