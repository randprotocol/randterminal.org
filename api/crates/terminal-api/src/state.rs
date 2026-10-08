use crate::{jobs::JobStore, ratelimit::RateLimiter, ApiConfig};
use std::sync::Arc;
use terminal_sandbox::{FakeRunner, RunOutput, RunSpec, Runner};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ApiConfig>,
    pub limiter: Arc<RateLimiter>,
    pub store: Arc<JobStore>,
    pub runs: Arc<Semaphore>,
}

impl AppState {
    pub fn new(config: ApiConfig, runner: Arc<dyn Runner>) -> Self {
        let config = Arc::new(config);
        Self { store: JobStore::new(config.clone(), runner), runs: Arc::new(Semaphore::new(config.max_runs.max(1))), limiter: Arc::new(RateLimiter::new()), config }
    }
    /// For router tests that never reach the sandbox: every tool step fails with status 127.
    pub fn for_tests(config: ApiConfig) -> Self {
        Self::new(config, Arc::new(FakeRunner::new(|_: &RunSpec| RunOutput { status: 127, stdout: String::new(), stderr: "no sandbox in tests".into(), timed_out: false })))
    }
}
