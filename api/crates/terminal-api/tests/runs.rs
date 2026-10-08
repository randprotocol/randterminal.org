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

fn evm_run(form: &terminal_core::views::EvmCallForm) -> Value { json!({ "call": { "lang": "evm", "function": form.function, "args": form.args, "caller": form.caller, "storage": form.storage } }) }

#[tokio::test(flavor = "multi_thread")]
async fn erc20_transfer_runs_and_matches_the_host_interpreter() {
    let (app, _s, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let form = evm_call::erc20_transfer_form();
    let (st, _, sum) = call(&app, json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(evm_run(&form)))).await;
    assert_eq!(st, StatusCode::CREATED, "{sum}");
    let code = hex::decode(std::fs::read_to_string(fixtures_dir().join("erc20.runtime.hex")).unwrap().trim()).unwrap();
    let (call_, _) = evm_call::build(&code, &form).unwrap();
    let (want, _, _) = call_.expected();
    assert_eq!(sum["outputs"], json!(want), "the RV32 image and the EVM interpreter agree");
    assert_eq!(sum["halted"], true);
    assert_eq!(sum["tier"], 18);
    let total = sum["cycles"].as_u64().unwrap();
    assert!(total > 60_000);
    let rid = sum["rid"].as_str().unwrap();

    let (_, _, again) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}"), None)).await;
    assert_eq!(again["cycles"], sum["cycles"]);
    let (_, _, page) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/cycles?from=0&count=10"), None)).await;
    assert_eq!(page["cycles"].as_array().unwrap().len(), 10);
    assert_eq!(page["total"], total);
    let (st, _, s0) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/state?at=0"), None)).await;
    assert_eq!(st, StatusCode::OK);
    assert!(s0["regs"].as_array().unwrap().iter().all(|r| r == 0));
    assert!(!s0["text"].as_str().unwrap().is_empty());
    let (st, _, _) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/state?at={}", total - 1), None)).await;
    assert_eq!(st, StatusCode::OK);
    let (st, _, _) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/state?at={total}"), None)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _, m) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/memory?at={total}&addr=0&words=16"), None)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(m["words"].as_array().unwrap().len(), 16);
    let (st, _, _) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/memory?at=0&addr=0&words=257"), None)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _, _) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/cycles?from=0&count=4097"), None)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, _, sys) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/syscalls"), None)).await;
    assert_eq!(sys.as_array().unwrap().last().unwrap()["kind"], "halt");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trap_is_a_summary_without_a_trace() {
    let (app, _s, _d) = test_app(fake_runner());
    let id = finished_erc20(&app).await;
    let mut body = evm_run(&evm_call::erc20_transfer_form());
    body["max_cycles"] = json!(1000);
    let (st, _, sum) = call(&app, json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(body))).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(sum["trap"], "OutOfCycles(1000)");
    assert_eq!(sum["trap_clk"], 1000);
    let rid = sum["rid"].as_str().unwrap();
    let (st, _, e) = call(&app, json_req("GET", &format!("/api/v1/runs/{rid}/cycles?from=0&count=1"), None)).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(e["message"].as_str().unwrap().contains("trapped"));
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_calls_are_400_and_unfinished_jobs_409() {
    let (app, _s, _d) = test_app(slow_runner());
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(erc20_job()))).await;
    let id = v["id"].as_str().unwrap().to_string();
    let (st, _, _) = call(&app, json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(evm_run(&evm_call::erc20_transfer_form())))).await;
    assert_eq!(st, StatusCode::CONFLICT);
    wait_job(&app, &id).await;
    let bad = json!({"call": {"lang": "evm", "function": "f(string)", "args": ["x"], "caller": "0x01"}});
    let (st, _, e) = call(&app, json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(bad))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(e["message"].as_str().unwrap().contains("string"));
    let sbpf = json!({"call": {"lang": "sbpf", "program_id": "11111111111111111111111111111111", "accounts": [], "data": "0x"}});
    let (st, _, _) = call(&app, json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(sbpf))).await;
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
    let (st, _, sum) = call(&app, json_req("POST", &format!("/api/v1/jobs/{id}/runs"), Some(body))).await;
    assert_eq!(st, StatusCode::CREATED, "{sum}");
    let want = rand_zkvm::sbpf::spl_transfer(250).expected().0;
    assert_eq!(sum["outputs"], json!(want));
    assert_eq!(sum["tier"], 20);
    assert_eq!(sum["public_words"], 27_151);
}
