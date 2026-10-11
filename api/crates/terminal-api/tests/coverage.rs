//! Unit-level behaviour that the router tests do not reach: configuration from the environment,
//! error and rejection shapes, the rate limiter's edges, job request validation, the data-cap and
//! sweep arithmetic, the small file helpers, and the failure paths of the pipeline.
mod common;
use axum::{
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use common::fake::*;
use common::*;
use std::net::SocketAddr;
use std::sync::Arc;
use terminal_api::{
    error::{json_rejections, AppError},
    jobs::{dir_size, read_regular, write_json, MAX_ELF_BYTES, MAX_TEXT_BYTES},
    ratelimit::{client_ip, Decision, RateLimiter},
    ApiConfig,
};
use terminal_core::views::{FileIn, JobOptions, Lang, NewJob};

// ---- ApiConfig::from_env ------------------------------------------------------------------------

/// The only test in this binary that touches the process environment.
#[test]
fn config_reads_the_environment_and_falls_back_on_garbage() {
    let keys = [
        ("API_HOST", "0.0.0.0"),
        ("API_PORT", "9191"),
        ("TERMINAL_DATA", "/srv/data"),
        ("TERMINAL_SAMPLES", "/srv/samples"),
        ("TERMINAL_SANDBOX_IMAGE", "terminal-sandbox:abc-def"),
        ("TERMINAL_MAX_PROGRAM_WORDS", "1000"),
        ("TERMINAL_MAX_CONCURRENT_BUILDS", "7"),
        ("TERMINAL_MAX_CONCURRENT_RUNS", "8"),
        ("TERMINAL_DATA_CAP_GB", "3"),
        ("TRUST_PROXY", "Yes"),
        ("TERMINAL_JOBS_RPM", "11"),
        ("TERMINAL_RUNS_RPM", "12"),
        ("TERMINAL_API_RPM", "13"),
        ("TERMINAL_RUNS_GET_RPM", "14"),
        ("TERMINAL_SCAN_RPM", "15"),
        ("TERMINAL_MAX_CONCURRENT_SCANS", "16"),
        ("TERMINAL_MAX_ACTIVE_JOBS_PER_IP", "17"),
        ("TERMINAL_MAX_QUEUED_JOBS", "18"),
    ];
    for (k, v) in keys {
        std::env::set_var(k, v);
    }
    let c = ApiConfig::from_env();
    assert_eq!(c.listen_addr, "0.0.0.0:9191".parse::<SocketAddr>().unwrap());
    assert_eq!(c.data_dir, std::path::PathBuf::from("/srv/data"));
    assert_eq!(c.samples_dir, std::path::PathBuf::from("/srv/samples"));
    assert_eq!(c.sandbox_image, "terminal-sandbox:abc-def");
    assert_eq!(c.max_program_words, 1000);
    assert_eq!((c.max_builds, c.max_runs), (7, 8));
    assert_eq!(c.data_cap_bytes, 3 << 30);
    assert!(c.trust_proxy);
    assert_eq!(
        (
            c.jobs_rpm,
            c.runs_rpm,
            c.api_rpm,
            c.runs_get_rpm,
            c.scan_rpm
        ),
        (11, 12, 13, 14, 15)
    );
    assert_eq!(
        (c.max_scans, c.max_active_jobs_per_ip, c.max_queued_jobs),
        (16, 17, 18)
    );
    // The TTL is not configurable.
    assert_eq!(c.job_ttl_secs, ApiConfig::default().job_ttl_secs);

    // TRUST_PROXY: only the falsy words turn it off, case-insensitively.
    for v in ["0", "false", "NO", "Off"] {
        std::env::set_var("TRUST_PROXY", v);
        assert!(!ApiConfig::from_env().trust_proxy, "{v}");
    }
    std::env::set_var("TRUST_PROXY", "1");
    assert!(ApiConfig::from_env().trust_proxy);

    // Unparseable numbers and a bad listen address fall back to the defaults; blanks count as unset.
    let d = ApiConfig::default();
    std::env::set_var("API_HOST", "not an address");
    std::env::set_var("TERMINAL_JOBS_RPM", "many");
    std::env::set_var("TERMINAL_DATA", "   ");
    let c = ApiConfig::from_env();
    assert_eq!(c.listen_addr, d.listen_addr);
    assert_eq!(c.jobs_rpm, d.jobs_rpm);
    assert_eq!(c.data_dir, d.data_dir);

    // A cap so large the byte count would overflow saturates.
    std::env::set_var("TERMINAL_DATA_CAP_GB", "18446744073709551615");
    assert_eq!(ApiConfig::from_env().data_cap_bytes, u64::MAX);

    for (k, _) in keys {
        std::env::remove_var(k);
    }
    let c = ApiConfig::from_env();
    assert_eq!(c.listen_addr, d.listen_addr);
    assert_eq!(c.data_cap_bytes, d.data_cap_bytes);
    assert!(!c.trust_proxy);
    assert!(c.sandbox_image.starts_with("terminal-sandbox:"));
}

// ---- errors -------------------------------------------------------------------------------------

async fn body_json(res: axum::response::Response) -> (StatusCode, HeaderMap, serde_json::Value) {
    use http_body_util::BodyExt;
    let (parts, body) = res.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (
        parts.status,
        parts.headers,
        serde_json::from_slice(&bytes).unwrap(),
    )
}

#[tokio::test]
async fn every_error_variant_has_its_status_and_tag() {
    let cases: Vec<(AppError, StatusCode, &str)> = vec![
        (
            AppError::NotFound("x".into()),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            AppError::BadRequest("x".into()),
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            AppError::PayloadTooLarge("x".into()),
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
        ),
        (
            AppError::Conflict("x".into()),
            StatusCode::CONFLICT,
            "conflict",
        ),
        (
            AppError::TooManyJobs("x".into()),
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_jobs",
        ),
        (
            AppError::Unavailable("x".into()),
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
        ),
    ];
    for (e, st, tag) in cases {
        let (s, h, v) = body_json(e.into_response()).await;
        assert_eq!(s, st);
        assert_eq!(v["error"], tag);
        assert_eq!(v["message"], "x");
        assert!(h.get("retry-after").is_none());
    }
}

#[tokio::test]
async fn rate_limited_carries_retry_after_and_internal_hides_the_detail() {
    let (s, h, v) = body_json(AppError::TooManyRequests { retry_after: 42 }.into_response()).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(h["retry-after"], "42");
    assert_eq!(v["error"], "rate_limited");
    assert_eq!(v["message"], "too many requests; retry in 42 seconds");

    let (s, _, v) = body_json(AppError::Internal("db path /secret".into()).into_response()).await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(v["error"], "internal");
    assert_eq!(v["message"], "internal error");
}

#[test]
fn io_and_json_errors_become_internal() {
    let e: AppError = std::io::Error::other("disk on fire").into();
    assert!(matches!(&e, AppError::Internal(m) if m == "disk on fire"));
    let j = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
    let msg = j.to_string();
    let e: AppError = j.into();
    assert!(matches!(&e, AppError::Internal(m) if *m == msg));
    assert_eq!(e.to_string(), format!("internal: {msg}"));
    assert_eq!(
        AppError::TooManyRequests { retry_after: 1 }.to_string(),
        "rate limited"
    );
}

fn plain(status: StatusCode, body: &str) -> axum::response::Response {
    (status, body.to_string()).into_response()
}

#[tokio::test]
async fn json_rejections_rewrites_plain_text_client_errors_only() {
    for (st, tag) in [
        (StatusCode::BAD_REQUEST, "bad_request"),
        (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
        (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type"),
        (StatusCode::UNPROCESSABLE_ENTITY, "unprocessable"),
    ] {
        let (s, _, v) = body_json(json_rejections(plain(st, "  boom \n")).await).await;
        assert_eq!(s, st);
        assert_eq!(v, serde_json::json!({"error": tag, "message": "boom"}));
        // An empty body falls back to the lower-cased reason phrase.
        let (_, _, v) = body_json(json_rejections(plain(st, "")).await).await;
        assert_eq!(v["message"], st.canonical_reason().unwrap().to_lowercase());
    }
    // Already JSON: untouched.
    let json = AppError::BadRequest("mine".into()).into_response();
    let (s, _, v) = body_json(json_rejections(json).await).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(v["message"], "mine");
    // Other statuses pass through as they are, text and all.
    use http_body_util::BodyExt;
    let res = json_rejections(plain(StatusCode::IM_A_TEAPOT, "tea")).await;
    assert_eq!(res.status(), StatusCode::IM_A_TEAPOT);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"tea");
}

// ---- rate limiter -------------------------------------------------------------------------------

#[test]
fn limiter_counts_per_id_and_resets_each_window() {
    let l = RateLimiter::new();
    assert_eq!(l.check("a", 2, 100), Decision::Allowed { remaining: 1 });
    assert_eq!(l.check("a", 2, 101), Decision::Allowed { remaining: 0 });
    // Window 60..120: retry when it ends.
    assert_eq!(l.check("a", 2, 105), Decision::Limited { retry_after: 15 });
    assert_eq!(l.check("a", 2, 119), Decision::Limited { retry_after: 1 });
    // Another id has its own count.
    assert_eq!(l.check("b", 2, 105), Decision::Allowed { remaining: 1 });
    // The next window starts fresh.
    assert_eq!(l.check("a", 2, 120), Decision::Allowed { remaining: 1 });
    // A limit of zero disables the bucket.
    for _ in 0..5 {
        assert_eq!(
            l.check("z", 0, 120),
            Decision::Allowed {
                remaining: u32::MAX
            }
        );
    }
}

#[test]
fn limiter_sweep_drops_only_expired_windows() {
    let l = RateLimiter::new();
    l.check("old", 1, 0); // window 0..60
    l.check("new", 1, 130); // window 120..180
    l.sweep(130);
    // "old" was swept: it starts a fresh count even within its former window.
    assert_eq!(l.check("old", 1, 10), Decision::Allowed { remaining: 0 });
    // "new" survived: still exhausted.
    assert_eq!(
        l.check("new", 1, 131),
        Decision::Limited { retry_after: 49 }
    );
    // Exactly at the window's end it is dropped.
    l.sweep(180);
    assert_eq!(l.check("new", 1, 180), Decision::Allowed { remaining: 0 });
}

#[tokio::test]
async fn limiter_sweeper_starts() {
    Arc::new(RateLimiter::new()).spawn_sweeper();
}

#[test]
fn client_ip_falls_back_to_the_peer() {
    let peer: SocketAddr = "203.0.113.9:5555".parse().unwrap();
    let mut h = HeaderMap::new();
    // No header, or the proxy is not trusted: the peer.
    assert_eq!(client_ip(&h, Some(peer), true), "203.0.113.9");
    h.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
    assert_eq!(client_ip(&h, Some(peer), false), "203.0.113.9");
    assert_eq!(client_ip(&h, Some(peer), true), "5.6.7.8");
    // An empty last hop is ignored.
    h.insert("x-forwarded-for", "1.2.3.4, ".parse().unwrap());
    assert_eq!(client_ip(&h, Some(peer), true), "203.0.113.9");
    // Nothing at all.
    assert_eq!(client_ip(&HeaderMap::new(), None, true), "unknown");
}

// ---- job validation (the store, not the router) -------------------------------------------------

fn file(name: &str, encoding: &str, content: &str) -> FileIn {
    FileIn {
        name: name.into(),
        encoding: encoding.into(),
        content: content.into(),
    }
}
fn job(lang: Lang, files: Vec<FileIn>) -> NewJob {
    NewJob {
        lang,
        files,
        options: JobOptions::default(),
    }
}

fn bad(e: terminal_api::error::ApiResult<String>) -> String {
    match e {
        Err(AppError::BadRequest(m)) => m,
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn job_requests_are_validated_before_anything_is_written() {
    let (_app, s, d) = test_app(fake_runner());
    let sol = |n: &str| file(n, "text", "contract A {}");

    // stage
    let mut j = job(Lang::Solidity, vec![sol("A.sol")]);
    j.options.stage = 0;
    assert_eq!(bad(s.store.create(j)), "stage must be 1 or 2, not 0");
    let mut j = job(Lang::Solidity, vec![sol("A.sol")]);
    j.options.stage = 3;
    assert_eq!(bad(s.store.create(j)), "stage must be 1 or 2, not 3");

    // names
    for n in ["", ".hidden", "a/b.sol", "a b.sol", &"x".repeat(65)] {
        let m = bad(s.store.create(job(Lang::Solidity, vec![sol(n)])));
        assert!(m.starts_with("bad file name"), "{n:?}: {m}");
    }
    let m = bad(s
        .store
        .create(job(Lang::Solidity, vec![sol("A.sol"), sol("A.sol")])));
    assert_eq!(m, "duplicate file name \"A.sol\"");

    // encodings
    let m = bad(s
        .store
        .create(job(Lang::Solidity, vec![file("A.sol", "rot13", "x")])));
    assert_eq!(m, "encoding \"rot13\"");
    let m = bad(s.store.create(job(
        Lang::SolanaElf,
        vec![file("program.so", "base64", "***")],
    )));
    assert!(m.starts_with("program.so: "), "{m}");

    // Solidity shape
    assert_eq!(
        bad(s.store.create(job(Lang::Solidity, vec![]))),
        "a Solidity job has 1 to 16 .sol files"
    );
    let many: Vec<FileIn> = (0..17).map(|i| sol(&format!("C{i}.sol"))).collect();
    assert_eq!(
        bad(s.store.create(job(Lang::Solidity, many))),
        "a Solidity job has 1 to 16 .sol files"
    );
    assert_eq!(
        bad(s.store.create(job(Lang::Solidity, vec![sol("A.txt")]))),
        "A.txt: Solidity jobs take .sol text files"
    );
    assert_eq!(
        bad(s
            .store
            .create(job(Lang::Solidity, vec![file("A.sol", "base64", "AAAA")]))),
        "A.sol: Solidity jobs take .sol text files"
    );

    // Solana Rust shape
    let rust = "a Solana Rust job is lib.rs and optionally Cargo.toml, as text";
    assert_eq!(
        bad(s
            .store
            .create(job(Lang::SolanaRust, vec![sol("Cargo.toml")]))),
        rust
    );
    assert_eq!(
        bad(s
            .store
            .create(job(Lang::SolanaRust, vec![sol("lib.rs"), sol("main.rs")]))),
        rust
    );
    assert_eq!(
        bad(s.store.create(job(
            Lang::SolanaRust,
            vec![file("lib.rs", "base64", "AAAA")]
        ))),
        rust
    );

    // Solana ELF shape
    let elf = "a Solana ELF job is one base64 file named program.so";
    assert_eq!(
        bad(s
            .store
            .create(job(Lang::SolanaElf, vec![file("a.so", "base64", "AAAA")]))),
        elf
    );
    assert_eq!(
        bad(s
            .store
            .create(job(Lang::SolanaElf, vec![sol("program.so")]))),
        elf
    );
    assert_eq!(bad(s.store.create(job(Lang::SolanaElf, vec![]))), elf);

    // Nothing reached the disk, and nothing is tracked as running.
    let jobs = d.path().join("jobs");
    assert_eq!(std::fs::read_dir(jobs).unwrap().count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn job_size_limits() {
    use base64::Engine;
    let (_app, s, _d) = test_app(fake_runner());
    let big = file("A.sol", "text", &"x".repeat(MAX_TEXT_BYTES + 1));
    match s.store.create(job(Lang::Solidity, vec![big])) {
        Err(AppError::PayloadTooLarge(m)) => {
            assert_eq!(
                m,
                format!(
                    "{} bytes of source; the limit is {MAX_TEXT_BYTES}",
                    MAX_TEXT_BYTES + 1
                )
            )
        }
        other => panic!("{other:?}"),
    }
    // The text cap is on the total across files.
    let half = "x".repeat(MAX_TEXT_BYTES / 2 + 1);
    let two = vec![file("A.sol", "text", &half), file("B.sol", "text", &half)];
    assert!(matches!(
        s.store.create(job(Lang::Solidity, two)),
        Err(AppError::PayloadTooLarge(_))
    ));
    // An ELF over its own cap.
    let b64 = base64::engine::general_purpose::STANDARD.encode(vec![0u8; MAX_ELF_BYTES + 1]);
    match s.store.create(job(
        Lang::SolanaElf,
        vec![file("program.so", "base64", &b64)],
    )) {
        Err(AppError::PayloadTooLarge(m)) => {
            assert_eq!(
                m,
                format!(
                    "{} bytes of ELF; the limit is {MAX_ELF_BYTES}",
                    MAX_ELF_BYTES + 1
                )
            )
        }
        other => panic!("{other:?}"),
    }
}

// ---- reading finished jobs ----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn unknown_ids_stages_and_files_are_404_and_unready_views_409() {
    let (_app, s, _d) = test_app(fake_runner());
    for bad_id in [
        "",
        "short",
        "../../etc/passwd",
        &"g".repeat(32),
        &"a".repeat(33),
    ] {
        assert!(
            matches!(s.store.view(bad_id), Err(AppError::NotFound(_))),
            "{bad_id}"
        );
        assert!(s.store.subscribe(bad_id).is_none());
    }
    let well_formed = "a".repeat(32);
    assert!(matches!(
        s.store.view(&well_formed),
        Err(AppError::NotFound(_))
    ));
    assert!(s.store.subscribe(&well_formed).is_none());
    // touch ignores ids that are not jobs.
    s.store.touch(&well_formed);
    s.store.touch("nope");
    assert_eq!(s.store.tracked(), 0);

    let id = s
        .store
        .create(job(
            Lang::Solidity,
            vec![file("ERC20.sol", "text", "contract ERC20 {}")],
        ))
        .unwrap();
    let v = wait_finished(&s.store, &id).await;
    assert!(v.finished);
    assert!(matches!(s.store.stage_view(&id, "bogus"), Err(AppError::NotFound(m)) if m == "bogus"));
    assert!(matches!(s.store.file(&id, "bogus"), Err(AppError::NotFound(m)) if m == "bogus"));
    // A known name whose file does not exist for this language.
    assert!(matches!(
        s.store.file(&id, "program.so"),
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        s.store.file(&id, "program.c"),
        Err(AppError::NotFound(_))
    ));
    // The names that do exist, with their content types.
    let (bytes, ty) = s.store.file(&id, "contract.c").unwrap();
    assert!(!bytes.is_empty());
    assert_eq!(ty, "text/plain; charset=utf-8");
    assert_eq!(s.store.file(&id, "abi.json").unwrap().1, "application/json");
    assert_eq!(
        s.store.file(&id, "image.bin").unwrap().1,
        "application/octet-stream"
    );
    assert!(s.store.tracked() >= 1);
}

#[test]
fn file_helpers() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("f.json");
    write_json(&f, &serde_json::json!({"a": 1})).unwrap();
    assert_eq!(read_regular(&f).unwrap(), br#"{"a":1}"#);
    // Rewriting replaces atomically and leaves no temporary behind.
    write_json(&f, &serde_json::json!({"a": 2})).unwrap();
    assert_eq!(read_regular(&f).unwrap(), br#"{"a":2}"#);
    assert!(!d.path().join("f.tmp").exists());
    // A stale temporary from a crashed write does not block it.
    std::fs::write(d.path().join("f.tmp"), "junk").unwrap();
    write_json(&f, &serde_json::json!({"a": 3})).unwrap();
    assert_eq!(read_regular(&f).unwrap(), br#"{"a":3}"#);

    // Not regular: missing, a directory, a link to a regular file.
    assert_eq!(
        read_regular(&d.path().join("nope")).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(read_regular(d.path())
        .unwrap_err()
        .to_string()
        .contains("is not a regular file"));
    #[cfg(unix)]
    {
        let l = d.path().join("link");
        std::os::unix::fs::symlink(&f, &l).unwrap();
        assert!(read_regular(&l).is_err());
    }

    // dir_size sums nested regular files; links are not followed; a missing dir is 0.
    let n = d.path().join("n");
    std::fs::create_dir_all(n.join("a/b")).unwrap();
    std::fs::write(n.join("x"), [0u8; 10]).unwrap();
    std::fs::write(n.join("a/y"), [0u8; 20]).unwrap();
    std::fs::write(n.join("a/b/z"), [0u8; 30]).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(d.path(), n.join("loop")).unwrap();
    let links = if cfg!(unix) {
        std::fs::symlink_metadata(n.join("loop")).unwrap().len()
    } else {
        0
    };
    assert_eq!(dir_size(&n), 60 + links);
    assert_eq!(dir_size(&d.path().join("missing")), 0);
}

// ---- data cap and sweeping ----------------------------------------------------------------------

fn sol_job() -> NewJob {
    job(
        Lang::Solidity,
        vec![file("ERC20.sol", "text", "contract ERC20 {}")],
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn check_space_refuses_over_the_cap_and_sweeps_in_the_background() {
    let (_app, s, _d) = test_app_with(fake_runner(), |c| c.data_cap_bytes = 1000);
    s.store.check_space().unwrap();
    s.store.add_usage(1000);
    s.store.check_space().unwrap(); // at the cap is not over it
    s.store.add_usage(1);
    let e = s.store.check_space().unwrap_err();
    assert!(
        matches!(&e, AppError::Unavailable(m) if m.contains("out of job storage")),
        "{e:?}"
    );
    // The background sweep re-measures the (empty) data dir and the cap opens again.
    for _ in 0..200 {
        if s.store.check_space().is_ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the background sweep never lowered the usage");
}

#[tokio::test(flavor = "multi_thread")]
async fn create_is_refused_over_the_cap_with_nothing_written() {
    let (_app, s, d) = test_app_with(fake_runner(), |c| c.data_cap_bytes = 10);
    s.store.add_usage(11);
    let before = std::fs::read_dir(d.path().join("jobs")).unwrap().count();
    let e = s.store.create(sol_job()).unwrap_err();
    assert!(matches!(e, AppError::Unavailable(_)), "{e:?}");
    assert_eq!(
        std::fs::read_dir(d.path().join("jobs")).unwrap().count(),
        before
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sweep_expires_by_ttl_and_keeps_fresh_and_kept_jobs() {
    let (_app, s, _d) = test_app_with(fake_runner(), |c| c.job_ttl_secs = 100);
    let mut ids = vec![];
    for _ in 0..3 {
        let id = s.store.create(sol_job()).unwrap();
        wait_finished(&s.store, &id).await;
        ids.push(id);
    }
    let now = terminal_api::ratelimit::unix_now();
    // Inside the TTL nothing goes.
    assert_eq!(s.store.sweep(now + 50), 0);
    assert!(ids.iter().all(|i| s.store.view(i).is_ok()));
    // Past it, all go except the one the request is about to use.
    assert_eq!(s.store.sweep_keeping(now + 1000, Some(&ids[1])), 2);
    assert!(s.store.view(&ids[1]).is_ok());
    for i in [&ids[0], &ids[2]] {
        assert!(matches!(s.store.view(i), Err(AppError::NotFound(_))));
        assert!(s.store.subscribe(i).is_none(), "its channel went with it");
    }
    // Without `keep`, the survivor is expired too; then there is nothing left.
    assert_eq!(s.store.sweep(now + 1000), 1);
    assert_eq!(s.store.sweep(now + 1000), 0);
}

// ---- pipeline failure paths ---------------------------------------------------------------------

use terminal_core::views::{JobView, StageStatus};
use terminal_sandbox::{RunOutput, RunSpec, Runner, SandboxError, Tool};

fn out(status: i32, stdout: &str, stderr: &str) -> RunOutput {
    RunOutput {
        status,
        stdout: stdout.into(),
        stderr: stderr.into(),
        timed_out: false,
    }
}

async fn run_with(
    lang: Lang,
    files: Vec<FileIn>,
    f: impl Fn(&RunSpec) -> RunOutput + Send + Sync + 'static,
) -> JobView {
    let (_app, s, _d) = test_app(Arc::new(terminal_sandbox::FakeRunner::new(f)));
    let id = s.store.create(job(lang, files)).unwrap();
    wait_finished(&s.store, &id).await
}

fn solidity() -> Vec<FileIn> {
    vec![file("ERC20.sol", "text", "contract ERC20 {}")]
}

#[tokio::test(flavor = "multi_thread")]
async fn docker_being_down_fails_the_stage_without_leaking_docker_output() {
    let v = run_with(Lang::Solidity, solidity(), |_| {
        out(
            125,
            "",
            "Unable to find image 'x' locally\ndocker: Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
        )
    })
    .await;
    assert!(v.finished);
    assert_eq!(v.compile.status, StageStatus::Failed);
    let e = v.compile.error.unwrap();
    assert_eq!(e.kind, "internal");
    assert_eq!(e.message, "the build sandbox is unavailable");
    assert!(
        v.console.is_empty(),
        "docker's complaint is for the operator log: {:?}",
        v.console
    );
    // Later stages never ran.
    assert_eq!(v.transpile.status, StageStatus::Queued);
    assert_eq!(v.build.status, StageStatus::Queued);
}

struct BrokenRunner;
#[async_trait::async_trait]
impl Runner for BrokenRunner {
    async fn run(&self, _: RunSpec) -> Result<RunOutput, SandboxError> {
        Err(SandboxError::Spawn("no docker binary".into()))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_runner_that_cannot_spawn_fails_the_stage_the_same_way() {
    let (_app, s, _d) = test_app(Arc::new(BrokenRunner));
    let id = s.store.create(job(Lang::Solidity, solidity())).unwrap();
    let v = wait_finished(&s.store, &id).await;
    let e = v.compile.error.unwrap();
    assert_eq!(
        (e.kind.as_str(), e.message.as_str()),
        ("internal", "the build sandbox is unavailable")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn solc_exiting_without_output_reports_its_status_and_stderr() {
    let v = run_with(Lang::Solidity, solidity(), |_| {
        out(1, "", "  solc: out of memory \n")
    })
    .await;
    let e = v.compile.error.unwrap();
    assert_eq!(e.kind, "compile");
    assert_eq!(
        e.message,
        "solc exited 1 without output: solc: out of memory"
    );
    // The tool's own stderr is shown on the console, trimmed of nothing but empties.
    assert_eq!(v.console.len(), 1);
    assert_eq!(v.console[0].stream, "stderr");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_cargo_build_reports_the_first_rustc_error_or_the_exit_status() {
    let rust = vec![file("lib.rs", "text", "fn broken(")];
    let v = run_with(Lang::SolanaRust, rust.clone(), |spec| {
        assert_eq!(spec.tool, Tool::BuildSbf);
        out(101, "", "")
    })
    .await;
    let e = v.compile.error.unwrap();
    assert_eq!(
        (e.kind.as_str(), e.message.as_str()),
        ("compile", "cargo build-sbf exited 101")
    );
    assert!(e.diagnostics.is_empty());

    let msg = serde_json::json!({
        "reason": "compiler-message",
        "message": {
            "level": "error",
            "message": "expected one of `)` or `,`",
            "rendered": "error: expected one of `)` or `,`",
            "spans": [{"file_name": "src/lib.rs", "line_start": 1, "column_start": 11, "is_primary": true}]
        }
    })
    .to_string();
    let v = run_with(Lang::SolanaRust, rust, move |_| out(101, &msg, "")).await;
    let e = v.compile.error.unwrap();
    assert_eq!(e.kind, "compile");
    assert!(!e.diagnostics.is_empty(), "{e:?}");
    assert_eq!(
        e.message,
        format!(
            "{}:{}:{}: {}",
            e.diagnostics[0].file,
            e.diagnostics[0].line,
            e.diagnostics[0].col,
            e.diagnostics[0].message
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_build_that_exits_zero_without_a_program_fails_the_compile_stage() {
    let v = run_with(Lang::SolanaRust, vec![file("lib.rs", "text", "")], |_| {
        out(0, "", "")
    })
    .await;
    let e = v.compile.error.unwrap();
    assert_eq!(
        (e.kind.as_str(), e.message.as_str()),
        ("compile", "cargo build-sbf exited 0")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bytes_that_are_not_an_elf_fail_the_compile_stage_before_any_sandbox_step() {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"definitely not an ELF file");
    let v = run_with(
        Lang::SolanaElf,
        vec![file("program.so", "base64", &b64)],
        |_| panic!("an uploaded ELF needs no sandbox step"),
    )
    .await;
    assert!(v.finished);
    assert_eq!(v.compile.status, StageStatus::Failed);
    let e = v.compile.error.unwrap();
    assert!(e.kind == "compile" || e.kind == "unsupported", "{}", e.kind);
    assert!(!e.message.is_empty());
    assert_eq!(v.transpile.status, StageStatus::Queued);
}

async fn transpile_of(runtime_hex: String, chain_id: u64) -> JobView {
    let (_app, s, _d) = test_app(Arc::new(terminal_sandbox::FakeRunner::new(
        move |spec: &RunSpec| {
            let o = fake_answer(spec);
            if spec.tool == Tool::Solc {
                let p = spec.work.join("output.json");
                let mut j: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
                j["contracts"]["ERC20.sol"]["ERC20"]["evm"]["deployedBytecode"]["object"] =
                    runtime_hex.clone().into();
                std::fs::write(&p, j.to_string()).unwrap();
            }
            o
        },
    )));
    let mut j = job(Lang::Solidity, solidity());
    j.options.chain_id = chain_id;
    let id = s.store.create(j).unwrap();
    wait_finished(&s.store, &id).await
}

#[tokio::test(flavor = "multi_thread")]
async fn code_over_the_size_limit_fails_the_transpile_stage_as_unsupported() {
    let v = transpile_of("00".repeat(24_577), 20).await;
    assert!(v.finished);
    assert_eq!(v.compile.status, StageStatus::Done, "{:?}", v.compile.error);
    assert_eq!(v.transpile.status, StageStatus::Failed);
    let e = v.transpile.error.unwrap();
    assert_eq!(e.kind, "unsupported");
    assert!(!e.message.is_empty());
    assert_eq!(v.build.status, StageStatus::Queued);
}

#[tokio::test(flavor = "multi_thread")]
async fn chainid_without_a_chain_id_fails_the_transpile_stage() {
    // 0x46 is CHAINID; with chain_id 0 the transpiler has nothing to substitute.
    let v = transpile_of("4600".into(), 0).await;
    assert_eq!(v.transpile.status, StageStatus::Failed);
    assert_eq!(v.transpile.error.unwrap().kind, "transpile");
    assert_eq!(v.build.status, StageStatus::Queued);
}

// ---- the binary ---------------------------------------------------------------------------------

/// Starts the real binary on a free loopback port and asks it for /health over plain TCP.
#[test]
fn the_binary_serves_health_on_the_configured_address() {
    use std::io::{Read, Write};
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let data = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_terminal-api"))
        .env("API_HOST", "127.0.0.1")
        .env("API_PORT", port.to_string())
        .env("TERMINAL_DATA", data.path())
        .env("TERMINAL_SANDBOX_IMAGE", "terminal-sandbox:test")
        .env(
            "TERMINAL_SAMPLES",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../../samples"),
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut body = String::new();
    for _ in 0..200 {
        if let Ok(mut c) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            c.set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            c.write_all(b"GET /api/v1/health HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
                .unwrap();
            let mut s = String::new();
            let _ = c.read_to_string(&mut s);
            body = s;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(body.starts_with("HTTP/1.1 200"), "{body}");
    assert!(
        body.contains("\"sandbox_image\":\"terminal-sandbox:test\""),
        "{body}"
    );
    assert!(body.contains("\"ok\":true"), "{body}");
}
