//! Writes the JSON the hermetic Playwright suite replays. Runs only with TERMINAL_RECORD=<dir>.
mod common;
use common::{fake::*, *};
use serde_json::{json, Value};

/// Wall-clock fields differ on every recording; zero them so a re-record only diffs on real changes.
fn scrub(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if matches!(k.as_str(), "created" | "started" | "ended") && x.is_number() {
                    *x = json!(0);
                } else {
                    scrub(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(scrub),
        _ => {}
    }
}

/// Sorted keys (serde_json's map is a BTreeMap here), pretty-printed, timestamps zeroed.
fn put(dir: &std::path::Path, name: &str, v: &Value) {
    let mut v = v.clone();
    scrub(&mut v);
    std::fs::write(dir.join(name), serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn record_e2e_fixtures() {
    let Ok(out) = std::env::var("TERMINAL_RECORD") else {
        eprintln!("skipping: TERMINAL_RECORD unset");
        return;
    };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (app, _s, _d) = test_app(fake_runner());
    let get = |p: String| {
        let app = app.clone();
        async move { call(&app, json_req("GET", &p, None)).await.2 }
    };
    put(&out, "samples.json", &get("/api/v1/samples".into()).await);
    let sample = get("/api/v1/samples/erc20".into()).await;
    put(&out, "sample-erc20.json", &sample);
    let (_, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(json!({"lang": "solidity", "files": [{"name": "ERC20.sol", "content": sample["files"][0]["content"]}], "options": {"contract": "ERC20", "chain_id": 13, "stage": 2}})))).await;
    let id = v["id"].as_str().unwrap().to_string();
    put(&out, "job.json", &wait_job(&app, &id).await);
    for s in ["compile", "transpile", "image"] {
        put(
            &out,
            &format!("{s}.json"),
            &get(format!("/api/v1/jobs/{id}/{s}")).await,
        );
    }
    let form = serde_json::to_value(terminal_core::evm_call::erc20_transfer_form()).unwrap();
    let mut call_ = form.clone();
    call_["lang"] = json!("evm");
    let (_, _, sum) = call(
        &app,
        json_req(
            "POST",
            &format!("/api/v1/jobs/{id}/runs"),
            Some(json!({"call": call_})),
        ),
    )
    .await;
    put(&out, "run.json", &sum);
    let rid = sum["rid"].as_str().unwrap();
    put(
        &out,
        "cycles-0.json",
        &get(format!("/api/v1/runs/{rid}/cycles?from=0&count=4096")).await,
    );
    let sys = get(format!("/api/v1/runs/{rid}/syscalls")).await;
    put(&out, "syscalls.json", &sys);
    let mut at: Vec<u64> = (0..32).collect();
    at.extend(
        sys.as_array()
            .unwrap()
            .iter()
            .take(64)
            .map(|s| s["clk"].as_u64().unwrap()),
    );
    let mut states = serde_json::Map::new();
    let mut mem = serde_json::Map::new();
    for a in at {
        let st = get(format!("/api/v1/runs/{rid}/state?at={a}")).await;
        let base = (st["cycle"]["mem_addr"].as_u64().unwrap_or(0)) & !15;
        mem.insert(
            a.to_string(),
            get(format!(
                "/api/v1/runs/{rid}/memory?at={a}&addr={base}&words=64"
            ))
            .await,
        );
        states.insert(a.to_string(), st);
    }
    put(&out, "states.json", &Value::Object(states));
    put(&out, "memory.json", &Value::Object(mem));
}
