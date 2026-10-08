use crate::{jobs::JobStore, ratelimit::RateLimiter, ApiConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminal_core::views::SampleView;
use terminal_sandbox::{FakeRunner, RunOutput, RunSpec, Runner};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ApiConfig>,
    pub limiter: Arc<RateLimiter>,
    pub store: Arc<JobStore>,
    pub runs: Arc<Semaphore>,
    pub samples: Arc<Vec<SampleView>>,
    pub sandbox: Arc<SandboxProbe>,
}

/// How long a sandbox availability answer is reused by /health.
pub const PROBE_TTL: Duration = Duration::from_secs(30);

/// `Runner::available`, cached for [`PROBE_TTL`] so /health stays cheap.
pub struct SandboxProbe {
    runner: Arc<dyn Runner>,
    last: tokio::sync::Mutex<Option<(Instant, bool)>>,
}

impl SandboxProbe {
    pub async fn available(&self) -> bool {
        let mut last = self.last.lock().await;
        if let Some((at, ok)) = *last {
            if at.elapsed() < PROBE_TTL {
                return ok;
            }
        }
        let ok = self.runner.available().await;
        *last = Some((Instant::now(), ok));
        ok
    }
}

impl AppState {
    pub fn new(config: ApiConfig, runner: Arc<dyn Runner>) -> Self {
        let config = Arc::new(config);
        let samples = Arc::new(
            crate::samples::load(&config.samples_dir).unwrap_or_else(|e| {
                tracing::warn!("samples: {e}");
                vec![]
            }),
        );
        Self {
            samples,
            sandbox: Arc::new(SandboxProbe {
                runner: runner.clone(),
                last: tokio::sync::Mutex::new(None),
            }),
            store: JobStore::new(config.clone(), runner),
            runs: Arc::new(Semaphore::new(config.max_runs.max(1))),
            limiter: Arc::new(RateLimiter::new()),
            config,
        }
    }
    /// For router tests that never reach the sandbox: every tool step fails with status 127.
    pub fn for_tests(config: ApiConfig) -> Self {
        Self::new(
            config,
            Arc::new(FakeRunner::new(|_: &RunSpec| RunOutput {
                status: 127,
                stdout: String::new(),
                stderr: "no sandbox in tests".into(),
                timed_out: false,
            })),
        )
    }
}
