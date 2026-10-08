//! Rand Terminal API (axum).
pub mod error;
pub mod handlers;
pub mod jobs;
pub mod ratelimit;
pub mod routes;
pub mod runs;
pub mod samples;
pub mod state;

use std::net::SocketAddr;
use std::path::PathBuf;

/// The circuits revision this binary links, from the submodule at build time (`build.rs`).
pub const CIRCUITS_REV: &str = env!("TERMINAL_CIRCUITS_REV");

#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub listen_addr: SocketAddr,
    pub data_dir: PathBuf,
    pub samples_dir: PathBuf,
    /// The sandbox image (`TERMINAL_SANDBOX_IMAGE`). Production must set it to the tag
    /// `sandbox/tag.sh` prints, `terminal-sandbox:<circuits rev>-<hash of sandbox/>`: the default,
    /// `terminal-sandbox:<circuits rev>`, cannot know the hash and is for development only.
    pub sandbox_image: String,
    pub max_program_words: usize,
    pub max_builds: usize,
    pub max_runs: usize,
    pub data_cap_bytes: u64,
    pub job_ttl_secs: u64,
    pub trust_proxy: bool,
    pub jobs_rpm: u32,
    pub runs_rpm: u32,
    pub api_rpm: u32,
    /// GETs under /runs/ (the debugger steps with state + memory per position).
    pub runs_get_rpm: u32,
    /// GETs of `/runs/:rid/next_src`, each a scan of up to 40 MB of trace.
    pub scan_rpm: u32,
    /// `next_src` scans in flight at once.
    pub max_scans: usize,
    /// Unfinished jobs one client may have at once.
    pub max_active_jobs_per_ip: usize,
    /// Unfinished jobs across all clients (queued for or in the sandbox).
    pub max_queued_jobs: usize,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            listen_addr: ([127, 0, 0, 1], 8080).into(),
            data_dir: PathBuf::from("data"),
            samples_dir: PathBuf::from("../samples"),
            sandbox_image: format!("terminal-sandbox:{CIRCUITS_REV}"),
            max_program_words: 65_535,
            max_builds: 2,
            max_runs: 4,
            data_cap_bytes: 20 << 30,
            job_ttl_secs: 3600,
            trust_proxy: false,
            jobs_rpm: 10,
            runs_rpm: 60,
            api_rpm: 600,
            runs_get_rpm: 6000,
            scan_rpm: 120,
            max_scans: 2,
            max_active_jobs_per_ip: 3,
            max_queued_jobs: 32,
        }
    }
}

impl ApiConfig {
    pub fn from_env() -> Self {
        let d = Self::default();
        let var = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        fn num<T: std::str::FromStr>(v: Option<String>, d: T) -> T {
            v.and_then(|s| s.parse().ok()).unwrap_or(d)
        }
        let host = var("API_HOST").unwrap_or_else(|| "127.0.0.1".into());
        let port: u16 = num(var("API_PORT"), 8080);
        Self {
            listen_addr: format!("{host}:{port}").parse().unwrap_or(d.listen_addr),
            data_dir: var("TERMINAL_DATA")
                .map(PathBuf::from)
                .unwrap_or(d.data_dir),
            samples_dir: var("TERMINAL_SAMPLES")
                .map(PathBuf::from)
                .unwrap_or(d.samples_dir),
            sandbox_image: var("TERMINAL_SANDBOX_IMAGE").unwrap_or(d.sandbox_image),
            max_program_words: num(var("TERMINAL_MAX_PROGRAM_WORDS"), d.max_program_words),
            max_builds: num(var("TERMINAL_MAX_CONCURRENT_BUILDS"), d.max_builds),
            max_runs: num(var("TERMINAL_MAX_CONCURRENT_RUNS"), d.max_runs),
            data_cap_bytes: num::<u64>(var("TERMINAL_DATA_CAP_GB"), 20).saturating_mul(1 << 30),
            job_ttl_secs: d.job_ttl_secs,
            trust_proxy: var("TRUST_PROXY")
                .map(|v| {
                    !matches!(
                        v.to_ascii_lowercase().as_str(),
                        "0" | "false" | "no" | "off"
                    )
                })
                .unwrap_or(false),
            jobs_rpm: num(var("TERMINAL_JOBS_RPM"), d.jobs_rpm),
            runs_rpm: num(var("TERMINAL_RUNS_RPM"), d.runs_rpm),
            api_rpm: num(var("TERMINAL_API_RPM"), d.api_rpm),
            runs_get_rpm: num(var("TERMINAL_RUNS_GET_RPM"), d.runs_get_rpm),
            scan_rpm: num(var("TERMINAL_SCAN_RPM"), d.scan_rpm),
            max_scans: num(var("TERMINAL_MAX_CONCURRENT_SCANS"), d.max_scans),
            max_active_jobs_per_ip: num(
                var("TERMINAL_MAX_ACTIVE_JOBS_PER_IP"),
                d.max_active_jobs_per_ip,
            ),
            max_queued_jobs: num(var("TERMINAL_MAX_QUEUED_JOBS"), d.max_queued_jobs),
        }
    }
}
