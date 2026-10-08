//! Fixed-window rate limiting per client IP, in memory, in five buckets: job submissions,
//! run submissions, `next_src` scans, other reads under /runs/ (the debugger), and everything else.
use crate::{error::AppError, state::AppState};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, Method},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WINDOW_SECS: u64 = 60;

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allowed { remaining: u32 },
    Limited { retry_after: u64 },
}

#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<String, (u64, u32)>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn check(&self, id: &str, limit: u32, now: u64) -> Decision {
        if limit == 0 {
            return Decision::Allowed {
                remaining: u32::MAX,
            };
        }
        let start = now - now % WINDOW_SECS;
        let mut map = self.windows.lock().unwrap_or_else(|p| p.into_inner());
        let e = map.entry(id.to_string()).or_insert((start, 0));
        if e.0 != start {
            *e = (start, 0);
        }
        if e.1 >= limit {
            return Decision::Limited {
                retry_after: start + WINDOW_SECS - now,
            };
        }
        e.1 += 1;
        Decision::Allowed {
            remaining: limit - e.1,
        }
    }
    pub fn sweep(&self, now: u64) {
        self.windows
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|_, (s, _)| *s + WINDOW_SECS > now);
    }
    pub fn spawn_sweeper(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await;
                self.sweep(unix_now());
            }
        });
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The client address: the LAST `X-Forwarded-For` hop when behind nginx (`trust_proxy`), else the peer.
/// nginx appends the real client address to whatever the client sent, so the last hop is the only
/// one a client cannot forge.
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return ip;
        }
    }
    peer.map(|p| p.ip().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// The client address `limit` resolved, for handlers that count per client (the job queue).
#[derive(Clone, Debug)]
pub struct ClientIp(pub String);

pub async fn limit(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    let ip = client_ip(req.headers(), peer, state.config.trust_proxy);
    let path = req.uri().path();
    // Nested under /api/v1, the path may arrive with or without the prefix.
    let run_read = path.starts_with("/runs/") || path.starts_with("/api/v1/runs/");
    let (bucket, rpm) = match (req.method(), path) {
        (&Method::POST, p) if p.ends_with("/jobs") => ("jobs", state.config.jobs_rpm),
        (&Method::POST, p) if p.ends_with("/runs") => ("runs", state.config.runs_rpm),
        (&Method::GET, p) if run_read && p.ends_with("/next_src") => {
            ("scan", state.config.scan_rpm)
        }
        (&Method::GET, _) if run_read => ("runs_get", state.config.runs_get_rpm),
        _ => ("api", state.config.api_rpm),
    };
    req.extensions_mut().insert(ClientIp(ip.clone()));
    match state
        .limiter
        .check(&format!("{bucket}:{ip}"), rpm, unix_now())
    {
        Decision::Allowed { .. } => next.run(req).await,
        Decision::Limited { retry_after } => {
            AppError::TooManyRequests { retry_after }.into_response()
        }
    }
}
