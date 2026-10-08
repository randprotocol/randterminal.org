mod common;
use common::fake::*;
use std::sync::Arc;
use terminal_api::{jobs::JobStore, ApiConfig};
use terminal_core::views::*;

fn store() -> (Arc<JobStore>, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let cfg = ApiConfig { data_dir: d.path().into(), ..ApiConfig::default() };
    (JobStore::new(Arc::new(cfg), fake_runner()), d)
}

fn sol(src: &str) -> NewJob {
    NewJob { lang: Lang::Solidity, files: vec![FileIn { name: "ERC20.sol".into(), encoding: "text".into(), content: src.into() }], options: JobOptions::default() }
}

#[tokio::test]
async fn solidity_runs_every_stage() {
    let (s, _d) = store();
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    for st in [&v.compile, &v.transpile, &v.build] { assert_eq!(st.status, StageStatus::Done, "{v:?}"); assert!(st.ended >= st.started); }
    let c: CompileView = serde_json::from_value(s.stage_view(&id, "compile").unwrap()).unwrap();
    let CompileView::Evm(c) = c else { panic!() };
    assert_eq!((c.contract.as_str(), c.size_bytes, c.blocks), ("ERC20", 1296, 74));
    let t: TranspileView = serde_json::from_value(s.stage_view(&id, "transpile").unwrap()).unwrap();
    assert!(t.c_source.contains("CHAINID 20"), "the default chain id reached the transpiler");
    let i: ImageView = serde_json::from_value(s.stage_view(&id, "image").unwrap()).unwrap();
    assert_eq!(i.words, 11_686);
    assert!(i.source_map.is_none());
    assert!(i.source_map_note.unwrap().contains("-g"));
    assert!(v.console.iter().any(|l| l.stage == StageName::Build && l.text.contains("wrote image.bin")));
    let (bytes, ty) = s.file(&id, "contract.c").unwrap();
    assert_eq!(ty, "text/plain; charset=utf-8");
    assert_eq!(String::from_utf8(bytes).unwrap(), t.c_source);
    assert!(s.file(&id, "../job.json").is_err());
}

#[tokio::test]
async fn a_compile_error_stops_the_pipeline_with_diagnostics() {
    let (s, _d) = store();
    let id = s.create(sol("SYNTAX_ERROR")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.compile.status, StageStatus::Failed);
    let e = v.compile.error.unwrap();
    assert_eq!(e.kind, "compile");
    assert_eq!((e.diagnostics[0].line, e.diagnostics[0].col), (1, 1));
    assert_eq!(v.transpile.status, StageStatus::Queued);
    assert_eq!(v.build.status, StageStatus::Queued);
    assert!(s.stage_view(&id, "transpile").is_err());
}

#[tokio::test]
async fn a_sandbox_timeout_is_reported_as_such() {
    let (s, _d) = store();
    let id = s.create(sol("SLOW")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.compile.error.unwrap().kind, "timeout");
}

#[tokio::test]
async fn solana_elf_skips_the_compiler() {
    use base64::Engine;
    let (s, _d) = store();
    let elf = std::fs::read(fixtures_dir().join("spl_token.so")).unwrap();
    let id = s.create(NewJob { lang: Lang::SolanaElf, files: vec![FileIn { name: "program.so".into(), encoding: "base64".into(), content: base64::engine::general_purpose::STANDARD.encode(&elf) }], options: JobOptions::default() }).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    let CompileView::Sbpf(c) = serde_json::from_value(s.stage_view(&id, "compile").unwrap()).unwrap() else { panic!() };
    assert_eq!(c.functions.len(), 30, "the SPL Token fixture scans to 30 functions (terminal-core asserts > 20)");
    let i: ImageView = serde_json::from_value(s.stage_view(&id, "image").unwrap()).unwrap();
    assert_eq!(i.words, 65_096);
}

#[tokio::test]
async fn solana_rust_errors_are_diagnostics() {
    let (s, _d) = store();
    let id = s.create(NewJob { lang: Lang::SolanaRust, files: vec![FileIn { name: "lib.rs".into(), encoding: "text".into(), content: "compile_error!(\"x\");".into() }], options: JobOptions::default() }).unwrap();
    let v = wait_finished(&s, &id).await;
    let e = v.compile.error.unwrap();
    assert_eq!(e.kind, "compile");
    assert_eq!(e.diagnostics[0].message, "boom");
}

#[test]
fn validation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _g = rt.enter();
    let (s, _d) = store();
    let bad = |j: NewJob| s.create(j).unwrap_err().to_string();
    assert!(bad(NewJob { lang: Lang::Solidity, files: vec![], options: JobOptions::default() }).contains("bad request"));
    assert!(bad(NewJob { lang: Lang::Solidity, files: vec![FileIn { name: "../x.sol".into(), encoding: "text".into(), content: "".into() }], options: JobOptions::default() }).contains("bad request"));
    assert!(bad(sol(&"x".repeat(512 * 1024 + 1))).contains("too large"));
    assert!(bad(NewJob { lang: Lang::SolanaRust, files: vec![FileIn { name: "main.rs".into(), encoding: "text".into(), content: "".into() }], options: JobOptions::default() }).contains("lib.rs"));
    let mut j = sol("contract A {}"); j.options.stage = 3;
    assert!(bad(j).contains("stage"));
}

#[tokio::test]
async fn sweep_drops_expired_jobs() {
    let (s, _d) = store();
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    wait_finished(&s, &id).await;
    assert_eq!(s.sweep(terminal_api::ratelimit::unix_now() + 10), 0);
    assert_eq!(s.sweep(terminal_api::ratelimit::unix_now() + 3601), 1);
    assert!(s.view(&id).is_err());
}

/// The container runs as uid 10001 while bind mounts keep host uids, so every stage work
/// directory (and what is in it) must be world-writable before the sandbox step runs.
#[cfg(unix)]
#[tokio::test]
async fn sandbox_work_dirs_are_world_writable() {
    use std::os::unix::fs::PermissionsExt;
    let (s, _d) = store();
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    let mode = |p: std::path::PathBuf| std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
    let dir = s.dir(&id);
    assert_eq!(mode(dir.join("compile")), 0o777);
    assert_eq!(mode(dir.join("compile/input.json")) & 0o666, 0o666);
    assert_eq!(mode(dir.join("build")), 0o777);
    assert_eq!(mode(dir.join("build/shim")), 0o777);
    assert_eq!(mode(dir.join("build/shim/src")), 0o777);
    assert_eq!(mode(dir.join("build/shim/contract.c")) & 0o666, 0o666);
    assert_eq!(mode(dir.join("build/shim/src/main.rs")) & 0o666, 0o666);
    assert!(dir.is_absolute(), "the docker runner rejects relative work paths");
}
