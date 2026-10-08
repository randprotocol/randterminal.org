mod common;
use axum::http::StatusCode;
use common::{fake::*, *};
use serde_json::{json, Value};
use terminal_core::{evm_call, sbpf_call};

async fn finished_erc20(app: &axum::Router) -> String {
    let (_, _, v) = call(app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(wait_job(app, &id).await["build"]["status"], "done");
    id
}

fn evm_run(form: &terminal_core::views::EvmCallForm) -> Value {
    json!({ "call": { "lang": "evm", "function": form.function, "args": form.args, "caller": form.caller, "storage": form.storage, "gas_limit": form.gas_limit } })
}

#[tokio::test(flavor = "multi_thread")]
async fn erc20_transfer_runs_and_matches_the_host_interpreter() {
    let (app, _s, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let form = evm_call::erc20_transfer_form();
    let (st, _, sum) = call(
        &app,
        json_req(
            "POST",
            &format!("/api/v1/jobs/{id}/runs"),
            Some(evm_run(&form)),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{sum}");
    let code = hex::decode(
        std::fs::read_to_string(fixtures_dir().join("erc20.runtime.hex"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    let (call_, _) = evm_call::build(&code, &form).unwrap();
    let (want, _, _) = call_.expected();
    assert_eq!(
        sum["outputs"],
        json!(want),
        "the RV32 image and the EVM interpreter agree"
    );
    assert_eq!(sum["halted"], true);
    assert_eq!(sum["tier"], 18);
    let total = sum["cycles"].as_u64().unwrap();
    assert!(total > 60_000);
    let rid = sum["rid"].as_str().unwrap();

    let (_, _, again) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}"), None)).await;
    assert_eq!(again["cycles"], sum["cycles"]);
    let (_, _, page) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/runs/{rid}/cycles?from=0&count=10"),
            None,
        ),
    )
    .await;
    assert_eq!(page["cycles"].as_array().unwrap().len(), 10);
    assert_eq!(page["total"], total);
    let (st, _, s0) = call(
        &app,
        json_req("GET", &format!("/api/v1/runs/{rid}/state?at=0"), None),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(s0["regs"].as_array().unwrap().iter().all(|r| r == 0));
    assert!(!s0["text"].as_str().unwrap().is_empty());
    let (st, _, _) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/runs/{rid}/state?at={}", total - 1),
            None,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _, _) = call(
        &app,
        json_req("GET", &format!("/api/v1/runs/{rid}/state?at={total}"), None),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _, m) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/runs/{rid}/memory?at={total}&addr=0&words=16"),
            None,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(m["words"].as_array().unwrap().len(), 16);
    let (st, _, _) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/runs/{rid}/memory?at=0&addr=0&words=257"),
            None,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _, _) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/runs/{rid}/cycles?from=0&count=4097"),
            None,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, _, sys) = call(
        &app,
        json_req("GET", &format!("/api/v1/runs/{rid}/syscalls"), None),
    )
    .await;
    assert_eq!(sys.as_array().unwrap().last().unwrap()["kind"], "halt");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trap_is_a_summary_without_a_trace() {
    let (app, _s, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let mut body = evm_run(&evm_call::erc20_transfer_form());
    body["max_cycles"] = json!(1000);
    let (st, _, sum) = call(
        &app,
        json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(body)),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(sum["trap"], "OutOfCycles(1000)");
    assert_eq!(sum["trap_clk"], 1000);
    let rid = sum["rid"].as_str().unwrap();
    let (st, _, e) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/runs/{rid}/cycles?from=0&count=1"),
            None,
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(
        e["message"],
        "the run ran out of cycles at 1000; there is no trace"
    );
    for ep in ["state?at=0", "memory?at=0&addr=0&words=1", "syscalls"] {
        let (st, _, _) = call(
            &app,
            json_req("GET", &format!("/api/v1/runs/{rid}/{ep}"), None),
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT, "{ep}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_calls_are_400_and_unfinished_jobs_409() {
    let (app, _s, _d) = test_app(slow_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap().to_string();
    let (st, _, _) = call(
        &app,
        json_req(
            "POST",
            &format!("/api/v1/jobs/{id}/runs"),
            Some(evm_run(&evm_call::erc20_transfer_form())),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    wait_job(&app, &id).await;
    let bad =
        json!({"call": {"lang": "evm", "function": "f(string)", "args": ["x"], "caller": "0x01"}});
    let (st, _, e) = call(
        &app,
        json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(bad)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(e["message"].as_str().unwrap().contains("string"));
    let sbpf = json!({"call": {"lang": "sbpf", "program_id": "11111111111111111111111111111111", "accounts": [], "data": "0x"}});
    let (st, _, _) = call(
        &app,
        json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(sbpf)),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _, _) = call(&app, json_req("GET", "/api/v1/runs/nope", None)).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn spl_transfer_runs_on_the_translated_image() {
    use base64::Engine;
    let (app, _s, _d) = test_app(fake_runner());
    let elf = std::fs::read(fixtures_dir().join("spl_token.so")).unwrap();
    let job = json!({"lang": "solana-elf", "files": [{"name": "program.so", "encoding": "base64", "content": base64::engine::general_purpose::STANDARD.encode(&elf)}]});
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(job))).await;
    let id = v["id"].as_str().unwrap().to_string();
    wait_job(&app, &id).await;
    let form = sbpf_call::spl_transfer_form(250);
    let mut body = json!({"call": serde_json::to_value(&form).unwrap()});
    body["call"]["lang"] = json!("sbpf");
    let (st, _, sum) = call(
        &app,
        json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(body)),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "{sum}");
    let want = rand_zkvm::sbpf::spl_transfer(250).expected().0;
    assert_eq!(sum["outputs"], json!(want));
    assert_eq!(sum["tier"], 20);
    assert_eq!(sum["public_words"], 27_151);
}

#[tokio::test(flavor = "multi_thread")]
async fn gas_limit_above_the_cap_is_400() {
    let (app, _s, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let mut form = evm_call::erc20_transfer_form();
    form.gas_limit = 30_000_001;
    let (st, _, e) = call(
        &app,
        json_req(
            "POST",
            &format!("/api/v1/jobs/{id}/runs"),
            Some(evm_run(&form)),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(e["message"].as_str().unwrap().contains("30000000"));
}

#[tokio::test(flavor = "multi_thread")]
async fn busy_slots_are_503() {
    let (app, state, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let _held: Vec<_> = (0..state.config.max_runs)
        .map(|_| state.runs.clone().try_acquire_owned().unwrap())
        .collect();
    let (st, _, _) = call(
        &app,
        json_req(
            "POST",
            &format!("/api/v1/jobs/{id}/runs"),
            Some(evm_run(&evm_call::erc20_transfer_form())),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
}

/// I4: "next source op" is one request. `next_src` scans the trace from `from` in `dir` for the
/// first cycle whose source pc is mapped and differs from the one at `from`.
#[tokio::test(flavor = "multi_thread")]
async fn next_src_finds_the_next_mapped_source_op() {
    let (app, state, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let (_, _, sum) = call(
        &app,
        json_req(
            "POST",
            &format!("/api/v1/jobs/{id}/runs"),
            Some(evm_run(&evm_call::erc20_transfer_form())),
        ),
    )
    .await;
    let rid = sum["rid"].as_str().unwrap().to_string();
    let total = sum["cycles"].as_u64().unwrap() as usize;
    let url = |q: &str| format!("/api/v1/runs/{rid}/next_src?{q}");
    // The fake sandbox has no -g build: no map, so nothing to search.
    let (st, _, e) = call(&app, json_req("GET", &url("from=0&dir=1"), None)).await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");

    // Map exactly one pc (cycle 100's) to C line 20, the source op at pc 7; every other pc to
    // line 1, which no source op precedes.
    let (_, _, page) = call(
        &app,
        json_req(
            "GET",
            &format!("/api/v1/runs/{rid}/cycles?from=0&count=4096"),
            None,
        ),
    )
    .await;
    let pcs: Vec<u64> = page["cycles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["pc"].as_u64().unwrap())
        .collect();
    let x = pcs[100];
    let jdir = state.store.dir(&id).unwrap();
    std::fs::write(
        jdir.join("build/source_map.json"),
        json!({ "pc_to_c": [[0, 1], [x, 20], [x + 4, 1]], "c_to_src": [[20, 7]] }).to_string(),
    )
    .unwrap();
    std::fs::write(
        jdir.join("transpile/op_map.json"),
        json!([{ "pc": 7, "line": 20 }]).to_string(),
    )
    .unwrap();

    let first = pcs.iter().position(|&p| p == x).unwrap();
    let (st, _, v) = call(&app, json_req("GET", &url("from=0&dir=1"), None)).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        (v["clk"].as_u64().unwrap() as usize, v["src_pc"].as_u64()),
        (first, Some(7))
    );
    // From a cycle at source op 7, the next *different* mapped op: there is none.
    let (st, _, _) = call(
        &app,
        json_req("GET", &url(&format!("from={first}&dir=1")), None),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // Backwards from the next unmapped cycle lands on the nearest earlier cycle at pc x.
    let after = (first + 1..).find(|&c| pcs[c] != x).unwrap();
    let (st, _, v) = call(
        &app,
        json_req("GET", &url(&format!("from={after}&dir=-1")), None),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["clk"].as_u64().unwrap() as usize, after - 1);
    for bad in ["from=0&dir=2", &format!("from={total}&dir=1"), "dir=1"] {
        let (st, _, e) = call(&app, json_req("GET", &url(bad), None)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}: {e}");
        assert!(e["error"].is_string(), "{bad}: {e}");
    }
}

/// I5: a job keeps its newest 8 runs; older run directories and their index files go.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_keeps_its_newest_eight_runs() {
    let (app, state, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let mut body = evm_run(&evm_call::erc20_transfer_form());
    body["max_cycles"] = json!(1000);
    let mut rids = vec![];
    for _ in 0..10 {
        let (st, _, sum) = call(
            &app,
            json_req(
                "POST",
                &format!("/api/v1/jobs/{id}/runs"),
                Some(body.clone()),
            ),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "{sum}");
        rids.push(sum["rid"].as_str().unwrap().to_string());
    }
    for (i, rid) in rids.iter().enumerate() {
        let (st, _, _) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}"), None)).await;
        assert_eq!(
            st,
            if i < 2 {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::OK
            },
            "run {i}"
        );
    }
    let jdir = state.store.dir(&id).unwrap();
    assert_eq!(std::fs::read_dir(jdir.join("runs")).unwrap().count(), 8);
    assert_eq!(
        std::fs::read_dir(state.config.data_dir.join("runs"))
            .unwrap()
            .count(),
        8
    );
}

/// I5: over the data cap, new runs are refused with 503.
#[tokio::test(flavor = "multi_thread")]
async fn runs_are_refused_over_the_data_cap() {
    let (app, _s, _d) = test_app_with(fake_runner(), |c| c.data_cap_bytes = 1);
    let id = finished_erc20(&app).await;
    let (st, _, v) = call(
        &app,
        json_req(
            "POST",
            &format!("/api/v1/jobs/{id}/runs"),
            Some(evm_run(&evm_call::erc20_transfer_form())),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert!(v["message"].as_str().unwrap().contains("storage"), "{v}");
}
