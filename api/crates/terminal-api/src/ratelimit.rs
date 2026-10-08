//! Fixed-window rate limiting per client IP, in memory, in three buckets: job submissions,
//! run submissions, and everything else.
use crate::{error::AppError, state::AppState};
use axum::{extract::{ConnectInfo, Request, State}, http::{HeaderMap, Method}, middleware::Next, response::{IntoResponse, Response}};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WINDOW_SECS: u64 = 60;

#[derive(Debug, PartialEq, Eq)]
pub enum Decision { Allowed { remaining: u32 }, Limited { retry_after: u64 } }

#[derive(Default)]
pub struct RateLimiter { windows: Mutex<HashMap<String, (u64, u32)>> }

impl RateLimiter {
    pub fn new() -> Self { Self::default() }
    pub fn check(&self, id: &str, limit: u32, now: u64) -> Decision {
        if limit == 0 { return Decision::Allowed { remaining: u32::MAX }; }
        let start = now - now % WINDOW_SECS;
        let mut map = self.windows.lock().unwrap_or_else(|p| p.into_inner());
        let e = map.entry(id.to_string()).or_insert((start, 0));
        if e.0 != start { *e = (start, 0); }
        if e.1 >= limit { return Decision::Limited { retry_after: start + WINDOW_SECS - now }; }
        e.1 += 1;
        Decision::Allowed { remaining: limit - e.1 }
    }
    pub fn sweep(&self, now: u64) { self.windows.lock().unwrap_or_else(|p| p.into_inner()).retain(|_, (s, _)| *s + WINDOW_SECS > now); }
    pub fn spawn_sweeper(self: Arc<Self>) {
        tokio::spawn(async move { loop { tokio::time::sleep(Duration::from_secs(300)).await; self.sweep(unix_now()); } });
    }
}

pub fn unix_now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

/// The client address: the first `X-Forwarded-For` hop when behind nginx (`trust_proxy`), else the peer.
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(ip) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) { return ip; }
    }
    peer.map(|p| p.ip().to_string()).unwrap_or_else(|| "unknown".into())
}

pub async fn limit(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let peer = req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    let ip = client_ip(req.headers(), peer, state.config.trust_proxy);
    let path = req.uri().path();
    let (bucket, rpm) = match (req.method() == Method::POST, path) {
        (true, p) if p.ends_with("/jobs") => ("jobs", state.config.jobs_rpm),
        (true, p) if p.ends_with("/runs") => ("runs", state.config.runs_rpm),
        _ => ("api", state.config.api_rpm),
    };
    match state.limiter.check(&format!("{bucket}:{ip}"), rpm, unix_now()) {
        Decision::Allowed { .. } => next.run(req).await,
        Decision::Limited { retry_after } => AppError::TooManyRequests { retry_after }.into_response(),
    }
}
