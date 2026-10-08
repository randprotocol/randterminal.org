//! A sandbox stand-in that answers each tool step from the checked-in fixtures.
#![allow(dead_code)]
use std::path::PathBuf;
use std::sync::Arc;
use terminal_api::jobs::JobStore;
use terminal_core::views::JobView;
use terminal_sandbox::{FakeRunner, RunOutput, RunSpec, Runner, SandboxError, Tool};
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn fixtures_dir() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../terminal-core/tests/fixtures") }

fn ok(stdout: &str) -> RunOutput { RunOutput { status: 0, stdout: stdout.into(), stderr: String::new(), timed_out: false } }

pub fn fake_runner() -> Arc<dyn Runner> { Arc::new(FakeRunner::new(fake_answer)) }

/// What the fake sandbox does for one step; other fakes wrap it.
pub fn fake_answer(spec: &RunSpec) -> RunOutput {
        let w = &spec.work;
        match spec.tool {
            Tool::Solc => {
                let input = std::fs::read_to_string(w.join("input.json")).unwrap();
                let out = if input.contains("SYNTAX_ERROR") {
                    serde_json::json!({"errors": [{"severity": "error", "message": "Expected ';' but got identifier", "sourceLocation": {"file": "ERC20.sol", "start": 0, "end": 1}}]})
                } else if input.contains("SLOW") {
                    return RunOutput { status: -1, stdout: String::new(), stderr: String::new(), timed_out: true };
                } else {
                    let hex = std::fs::read_to_string(fixtures_dir().join("erc20.runtime.hex")).unwrap();
                    serde_json::json!({"contracts": {"ERC20.sol": {"ERC20": {"abi": [{"type": "function", "name": "transfer", "inputs": [{"name": "to", "type": "address"}, {"name": "amount", "type": "uint256"}], "outputs": [{"name": "", "type": "bool"}], "stateMutability": "nonpayable"}], "evm": {"bytecode": {"object": "6080"}, "deployedBytecode": {"object": hex.trim()}}}}}})
                };
                std::fs::write(w.join("output.json"), out.to_string()).unwrap();
                ok("")
            }
            Tool::BuildSbf => {
                let src = std::fs::read_to_string(w.join("src/lib.rs")).unwrap();
                if src.contains("compile_error") {
                    return RunOutput { status: 101, stdout: r#"{"reason":"compiler-message","message":{"level":"error","message":"boom","spans":[{"file_name":"src/lib.rs","line_start":1,"column_start":1,"is_primary":true}]}}"#.into(), stderr: "error: could not compile".into(), timed_out: false };
                }
                std::fs::copy(fixtures_dir().join("spl_token.so"), w.join("program.so")).unwrap();
                ok("Finished release")
            }
            Tool::RandGuestBuild { debug: false } => {
                let fixture = if w.join("shim/contract.c").exists() { "erc20.image.bin" } else { "spl.image.bin" };
                std::fs::copy(fixtures_dir().join(fixture), w.join("image.bin")).unwrap();
                ok("wrote image.bin")
            }
            Tool::RandGuestBuild { debug: true } => RunOutput { status: 66, stdout: String::new(), stderr: "could not add -g to build.rs".into(), timed_out: false },
        }
}

pub async fn wait_finished(store: &JobStore, id: &str) -> JobView {
    for _ in 0..600 {
        let v = store.view(id).unwrap();
        if v.finished { return v; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("job {id} did not finish in 30 s");
}

/// Holds every step until `open` is called, then answers like `fake_answer`.
pub struct GatedRunner { pub gate: tokio::sync::Semaphore }
impl GatedRunner { pub fn new() -> Arc<Self> { Arc::new(Self { gate: tokio::sync::Semaphore::new(0) }) } pub fn open(&self) { self.gate.add_permits(1 << 20); } }
#[async_trait::async_trait]
impl Runner for GatedRunner {
    async fn run(&self, spec: RunSpec) -> Result<RunOutput, SandboxError> {
        let _p = self.gate.acquire().await.unwrap();
        Ok(fake_answer(&spec))
    }
}

/// Answers like `fake_answer` after 50 ms, recording the most steps that were ever in flight at once.
#[derive(Default)]
pub struct CountingRunner { pub active: AtomicUsize, pub peak: AtomicUsize, pub calls: AtomicUsize }
#[async_trait::async_trait]
impl Runner for CountingRunner {
    async fn run(&self, spec: RunSpec) -> Result<RunOutput, SandboxError> {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let out = fake_answer(&spec);
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(out)
    }
}

/// The fake runner, with solc sleeping 400 ms so a job is observably in flight.
pub fn slow_runner() -> Arc<dyn Runner> {
    Arc::new(FakeRunner::new(|spec: &RunSpec| {
        if spec.tool == Tool::Solc { std::thread::sleep(std::time::Duration::from_millis(400)); }
        fake_answer(spec)
    }))
}
