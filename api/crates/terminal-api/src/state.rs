use crate::{ratelimit::RateLimiter, ApiConfig};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ApiConfig>,
    pub limiter: Arc<RateLimiter>,
    pub runs: Arc<Semaphore>,
}

impl AppState {
    pub fn for_tests(config: ApiConfig) -> Self {
        Self { runs: Arc::new(Semaphore::new(config.max_runs)), config: Arc::new(config), limiter: Arc::new(RateLimiter::new()) }
    }
}
