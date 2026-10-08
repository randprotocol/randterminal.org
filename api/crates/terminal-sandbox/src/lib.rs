//! Runs the untrusted tool steps (solc, cargo build-sbf, rand-guest build) in a container with no
//! network, capped memory, cpu and pids, a read-only root and one writable bind mount. One fixed
//! entrypoint per tool; nothing from the user reaches the command line.
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub const MAX_CAPTURE: usize = 1 << 20;

/// How long, after a timeout kill, the capture readers get to drain before they are abandoned.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

/// Upper bound on the `docker kill` issued after a timeout, so a hung daemon cannot hang `run`.
const KILL_GRACE: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Solc,
    BuildSbf,
    RandGuestBuild { debug: bool },
}

#[derive(Clone, Debug)]
pub struct RunSpec {
    pub tool: Tool,
    pub work: PathBuf,
    pub timeout: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct RunOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum SandboxError {
    #[error("spawn: {0}")]
    Spawn(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[async_trait]
pub trait Runner: Send + Sync {
    async fn run(&self, spec: RunSpec) -> Result<RunOutput, SandboxError>;
}

pub fn timeout_for(tool: &Tool) -> Duration {
    match tool {
        Tool::Solc => Duration::from_secs(30),
        Tool::BuildSbf => Duration::from_secs(300),
        Tool::RandGuestBuild { .. } => Duration::from_secs(180),
    }
}

pub fn mount_point(tool: &Tool) -> &'static str {
    match tool {
        Tool::RandGuestBuild { .. } => "/circuits/jobs/job",
        _ => "/work",
    }
}

fn entry(tool: &Tool) -> Vec<String> {
    match tool {
        Tool::Solc => vec!["solc".into()],
        Tool::BuildSbf => vec!["build-sbf".into()],
        Tool::RandGuestBuild { debug } => {
            vec!["rand-guest-build".into(), if *debug { "debug" } else { "release" }.into()]
        }
    }
}

pub struct DockerRunner {
    pub program: String,
    pub image: String,
    pub memory: String,
    pub cpus: String,
}

impl DockerRunner {
    pub fn new(image: impl Into<String>) -> Self {
        Self { program: "docker".into(), image: image.into(), memory: "4g".into(), cpus: "2".into() }
    }

    pub fn args(&self, spec: &RunSpec, name: &str) -> Vec<String> {
        let mut a: Vec<String> = [
            "run", "--rm", "--name", name, "--network", "none",
            "--memory", &self.memory, "--memory-swap", &self.memory, "--cpus", &self.cpus, "--pids-limit", "512",
            "--read-only", "--tmpfs", "/tmp:rw,exec,size=2g", "--security-opt", "no-new-privileges", "--cap-drop", "ALL",
            "--user", "10001:10001",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        a.push("-v".into());
        a.push(format!("{}:{}:rw", spec.work.display(), mount_point(&spec.tool)));
        a.push(self.image.clone());
        a.extend(entry(&spec.tool));
        a
    }
}

type Capture = Arc<Mutex<Vec<u8>>>;

/// Reads to EOF into `buf`, keeping at most `MAX_CAPTURE` bytes; keeps draining so the child never blocks on a full pipe.
async fn read_capped(mut r: impl tokio::io::AsyncRead + Unpin, buf: Capture) {
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                let room = MAX_CAPTURE.saturating_sub(b.len());
                b.extend_from_slice(&chunk[..n.min(room)]);
            }
        }
    }
}

/// Lossy UTF-8, truncated on a char boundary so the string never exceeds `MAX_CAPTURE` bytes.
fn finish(buf: &Capture) -> String {
    let bytes = buf.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut s = String::from_utf8_lossy(&bytes).into_owned();
    if s.len() > MAX_CAPTURE {
        let mut end = MAX_CAPTURE;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

/// Waits for a reader; after a timeout the pipe may be held open by a grandchild, so bound the wait and abandon it.
/// Whatever was captured so far stays in the shared buffer.
async fn join_reader(h: tokio::task::JoinHandle<()>, bounded: bool) {
    if !bounded {
        let _ = h.await;
        return;
    }
    let abort = h.abort_handle();
    if tokio::time::timeout(DRAIN_GRACE, h).await.is_err() {
        abort.abort();
    }
}

/// Kills the container by name if the `run` future is dropped before it finishes.
struct ContainerGuard {
    program: String,
    name: String,
    armed: bool,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::process::Command::new(&self.program)
                .args(["kill", &self.name])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}

fn check_work(p: &std::path::Path) -> Result<(), SandboxError> {
    let s = p.to_string_lossy();
    if !p.is_absolute() || s.contains([':', ',', '\n']) {
        return Err(SandboxError::Spawn(format!("unsafe work path: {s}")));
    }
    Ok(())
}

#[async_trait]
impl Runner for DockerRunner {
    async fn run(&self, spec: RunSpec) -> Result<RunOutput, SandboxError> {
        check_work(&spec.work)?;
        let name = format!("terminal-{:016x}", rand_id());
        let mut child = Command::new(&self.program)
            .args(self.args(&spec, &name))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| SandboxError::Spawn(format!("{}: {e}", self.program)))?;
        let mut guard = ContainerGuard { program: self.program.clone(), name: name.clone(), armed: true };
        let (out_buf, err_buf): (Capture, Capture) = Default::default();
        let out = tokio::spawn(read_capped(child.stdout.take().expect("piped stdout"), out_buf.clone()));
        let err = tokio::spawn(read_capped(child.stderr.take().expect("piped stderr"), err_buf.clone()));
        let (status, timed_out) = match tokio::time::timeout(spec.timeout, child.wait()).await {
            Ok(s) => (s?.code().unwrap_or(-1), false),
            Err(_) => {
                // Stop the container itself; killing the CLI process alone would leave it running.
                let _ = tokio::time::timeout(KILL_GRACE, Command::new(&self.program).args(["kill", &name]).output()).await;
                let _ = child.kill().await;
                (-1, true)
            }
        };
        guard.armed = false;
        join_reader(out, timed_out).await;
        join_reader(err, timed_out).await;
        Ok(RunOutput { status, stdout: finish(&out_buf), stderr: finish(&err_buf), timed_out })
    }
}

fn rand_id() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    h.finish()
}

pub struct FakeRunner<F: Fn(&RunSpec) -> RunOutput + Send + Sync> {
    f: F,
}

impl<F: Fn(&RunSpec) -> RunOutput + Send + Sync> FakeRunner<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

#[async_trait]
impl<F: Fn(&RunSpec) -> RunOutput + Send + Sync> Runner for FakeRunner<F> {
    async fn run(&self, spec: RunSpec) -> Result<RunOutput, SandboxError> {
        Ok((self.f)(&spec))
    }
}
