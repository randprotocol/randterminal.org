mod common;
use axum::http::StatusCode;
use common::{fake::*, *};
use terminal_core::views::{CallForm, SampleView};

#[tokio::test]
async fn samples_list_and_load() {
    let (app, _s, _d) = test_app(fake_runner());
    let (st, _, list) = call(&app, json_req("GET", "/api/v1/samples", None)).await;
    assert_eq!(st, StatusCode::OK);
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["counter", "erc20", "solana-counter", "spl-token"]);
    let (_, _, v) = call(&app, json_req("GET", "/api/v1/samples/erc20", None)).await;
    let s: SampleView = serde_json::from_value(v).unwrap();
    assert_eq!(
        s.files[0].content,
        std::fs::read_to_string(fixtures_dir().join("ERC20.sol")).unwrap()
    );
    assert!(matches!(s.meta.default_call, Some(CallForm::Evm(_))));
    let (_, _, v) = call(&app, json_req("GET", "/api/v1/samples/spl-token", None)).await;
    let s: SampleView = serde_json::from_value(v).unwrap();
    assert_eq!(s.files[0].encoding, "base64");
    assert!(matches!(s.meta.default_call, Some(CallForm::Sbpf(_))));
    let (st, _, _) = call(&app, json_req("GET", "/api/v1/samples/nope", None)).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[test]
fn every_default_call_is_well_formed() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../samples");
    for s in terminal_api::samples::load(&dir).unwrap() {
        match s.meta.default_call {
            Some(CallForm::Sbpf(f)) => {
                terminal_core::sbpf_call::parse_key(&f.program_id)
                    .unwrap_or_else(|e| panic!("{}: {e}", s.meta.id));
                for a in &f.accounts {
                    terminal_core::sbpf_call::parse_key(&a.key).unwrap();
                    terminal_core::sbpf_call::parse_key(&a.owner).unwrap();
                }
            }
            Some(CallForm::Evm(f)) => {
                terminal_core::evm_call::encode_calldata(&f.function, &f.args).unwrap();
            }
            None => panic!("{} has no default call", s.meta.id),
        }
    }
}

#[test]
fn a_broken_sample_does_not_blank_the_rest() {
    let d = tempfile::tempdir().unwrap();
    let ok = d.path().join("ok");
    std::fs::create_dir(&ok).unwrap();
    std::fs::write(
        ok.join("meta.json"),
        r#"{"id":"ok","title":"Ok","lang":"solidity","files":["a.sol"]}"#,
    )
    .unwrap();
    std::fs::write(ok.join("a.sol"), "contract A {}").unwrap();
    let bad = d.path().join("bad");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(bad.join("meta.json"), "{ not json").unwrap();
    std::fs::create_dir(d.path().join("empty")).unwrap();
    std::fs::write(d.path().join("stray.txt"), "x").unwrap();
    let got = terminal_api::samples::load(d.path()).unwrap();
    assert_eq!(
        got.iter().map(|s| s.meta.id.as_str()).collect::<Vec<_>>(),
        ["ok"]
    );
}

#[test]
fn bundled_programs_are_the_solana_elf_samples_hashed_at_load() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../samples");
    let samples = terminal_api::samples::load(&dir).unwrap();
    let b = terminal_api::samples::bundled_programs(&samples);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].id, "spl-token");
    let so = std::fs::read(dir.join("spl-token/spl_token.so")).unwrap();
    assert_eq!(*b[0].bytes, so);
    assert_eq!(
        hex::encode(b[0].sha256),
        "8190d3f7ceb6cb7a7a8d8924bff89f9f611e15ce1f806f2b6237f3311a98f697"
    );
}

/// The web app sends the sample's id, never its bytes; the job runs the bundled program.
#[tokio::test(flavor = "multi_thread")]
async fn a_solana_elf_job_by_sample_id_runs_the_bundled_program() {
    let (app, _s, _d) = test_app(fake_runner());
    let job = serde_json::json!({"lang": "solana-elf", "sample": "spl-token"});
    let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(job))).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let j = wait_job(&app, &id).await;
    assert_eq!(j["build"]["status"], "done", "{j}");
    let (_, _, c) = call(
        &app,
        json_req("GET", &format!("/api/v1/jobs/{id}/compile"), None),
    )
    .await;
    assert_eq!(c["lang"], "sbpf");
    assert_eq!(
        c["elf_bytes"],
        std::fs::metadata(fixtures_dir().join("spl_token.so"))
            .unwrap()
            .len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_uploaded_elf_is_a_400_over_http() {
    use base64::Engine;
    let (app, _s, _d) = test_app(fake_runner());
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"\x7fELF not really");
    let job = serde_json::json!({"lang": "solana-elf", "files": [{"name": "program.so", "encoding": "base64", "content": b64}]});
    let (st, _, v) = call(&app, json_req("POST", "/api/v1/jobs", Some(job))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"], "bad_request");
    assert!(v["message"]
        .as_str()
        .unwrap()
        .contains("uploaded programs are not accepted"));
}
