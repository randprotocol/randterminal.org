mod common;
use common::fake::*;
use std::sync::Arc;
use terminal_api::{jobs::JobStore, ApiConfig};
use terminal_core::views::*;

fn store() -> (Arc<JobStore>, tempfile::TempDir) {
    store_with(fake_runner())
}

fn store_with(runner: Arc<dyn terminal_sandbox::Runner>) -> (Arc<JobStore>, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let cfg = ApiConfig {
        data_dir: d.path().into(),
        ..ApiConfig::default()
    };
    (JobStore::new(Arc::new(cfg), runner), d)
}

fn store_cfg(
    runner: Arc<dyn terminal_sandbox::Runner>,
    f: impl FnOnce(&mut ApiConfig),
) -> (Arc<JobStore>, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let mut cfg = ApiConfig {
        data_dir: d.path().into(),
        ..ApiConfig::default()
    };
    f(&mut cfg);
    (JobStore::new(Arc::new(cfg), runner), d)
}

fn rust_job(src: &str) -> NewJob {
    NewJob {
        lang: Lang::SolanaRust,
        files: vec![FileIn {
            name: "lib.rs".into(),
            encoding: "text".into(),
            content: src.into(),
        }],
        options: JobOptions::default(),
    }
}

fn sol(src: &str) -> NewJob {
    NewJob {
        lang: Lang::Solidity,
        files: vec![FileIn {
            name: "ERC20.sol".into(),
            encoding: "text".into(),
            content: src.into(),
        }],
        options: JobOptions::default(),
    }
}

#[tokio::test]
async fn solidity_runs_every_stage() {
    let (s, _d) = store();
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    for st in [&v.compile, &v.transpile, &v.build] {
        assert_eq!(st.status, StageStatus::Done, "{v:?}");
        assert!(st.ended >= st.started);
    }
    let c: CompileView = serde_json::from_value(s.stage_view(&id, "compile").unwrap()).unwrap();
    let CompileView::Evm(c) = c else { panic!() };
    assert_eq!(
        (c.contract.as_str(), c.size_bytes, c.blocks),
        ("ERC20", 1296, 74)
    );
    let t: TranspileView = serde_json::from_value(s.stage_view(&id, "transpile").unwrap()).unwrap();
    assert!(
        t.c_source.contains("CHAINID 20"),
        "the default chain id reached the transpiler"
    );
    let i: ImageView = serde_json::from_value(s.stage_view(&id, "image").unwrap()).unwrap();
    assert_eq!(i.words, 11_686);
    assert!(i.source_map.is_none());
    assert!(i.source_map_note.unwrap().contains("-g"));
    assert!(v
        .console
        .iter()
        .any(|l| l.stage == StageName::Build && l.text.contains("wrote image.bin")));
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
    let id = s
        .create(NewJob {
            lang: Lang::SolanaElf,
            files: vec![FileIn {
                name: "program.so".into(),
                encoding: "base64".into(),
                content: base64::engine::general_purpose::STANDARD.encode(&elf),
            }],
            options: JobOptions::default(),
        })
        .unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    let CompileView::Sbpf(c) =
        serde_json::from_value(s.stage_view(&id, "compile").unwrap()).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        c.functions.len(),
        30,
        "the SPL Token fixture scans to 30 functions (terminal-core asserts > 20)"
    );
    let i: ImageView = serde_json::from_value(s.stage_view(&id, "image").unwrap()).unwrap();
    assert_eq!(i.words, 65_096);
}

#[tokio::test]
async fn solana_rust_errors_are_diagnostics() {
    let (s, _d) = store();
    let id = s
        .create(NewJob {
            lang: Lang::SolanaRust,
            files: vec![FileIn {
                name: "lib.rs".into(),
                encoding: "text".into(),
                content: "compile_error!(\"x\");".into(),
            }],
            options: JobOptions::default(),
        })
        .unwrap();
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
    assert!(bad(NewJob {
        lang: Lang::Solidity,
        files: vec![],
        options: JobOptions::default()
    })
    .contains("bad request"));
    assert!(bad(NewJob {
        lang: Lang::Solidity,
        files: vec![FileIn {
            name: "../x.sol".into(),
            encoding: "text".into(),
            content: "".into()
        }],
        options: JobOptions::default()
    })
    .contains("bad request"));
    assert!(bad(sol(&"x".repeat(512 * 1024 + 1))).contains("too large"));
    assert!(bad(NewJob {
        lang: Lang::SolanaRust,
        files: vec![FileIn {
            name: "main.rs".into(),
            encoding: "text".into(),
            content: "".into()
        }],
        options: JobOptions::default()
    })
    .contains("lib.rs"));
    let mut j = sol("contract A {}");
    j.options.stage = 3;
    assert!(bad(j).contains("stage"));
    let mut j = sol("contract A {}");
    j.files.push(j.files[0].clone());
    assert!(bad(j).contains("duplicate"));
    use base64::Engine;
    let elf =
        base64::engine::general_purpose::STANDARD
            .encode(vec![0u8; terminal_api::jobs::MAX_ELF_BYTES + 1]);
    assert!(bad(NewJob {
        lang: Lang::SolanaElf,
        files: vec![FileIn {
            name: "program.so".into(),
            encoding: "base64".into(),
            content: elf
        }],
        options: JobOptions::default()
    })
    .contains("too large"));
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

/// The container runs as uid 10001 while bind mounts keep host uids, so each step's sandbox/
/// directory (and what is in it) must be world-writable; the host-owned directories must not be.
#[cfg(unix)]
#[tokio::test]
async fn only_the_sandbox_dirs_are_world_writable() {
    use std::os::unix::fs::PermissionsExt;
    let (s, _d) = store();
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    let mode = |p: std::path::PathBuf| std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
    let dir = s.dir(&id).unwrap();
    assert!(
        dir.is_absolute(),
        "the docker runner rejects relative work paths"
    );
    assert_eq!(mode(dir.join("compile/sandbox")), 0o777);
    assert_eq!(mode(dir.join("compile/sandbox/input.json")) & 0o666, 0o666);
    assert_eq!(mode(dir.join("build/sandbox")), 0o777);
    assert_eq!(mode(dir.join("build/sandbox/shim")), 0o777);
    assert_eq!(mode(dir.join("build/sandbox/shim/src")), 0o777);
    assert_eq!(
        mode(dir.join("build/sandbox/shim/contract.c")) & 0o666,
        0o666
    );
    assert_eq!(
        mode(dir.join("build/sandbox/shim/src/main.rs")) & 0o666,
        0o666
    );
    for host in [
        "",
        "compile",
        "transpile",
        "build",
        "source",
        "compile/abi.json",
        "build/image.bin",
        "job.json",
    ] {
        assert_eq!(
            mode(dir.join(host)) & 0o002,
            0,
            "{host} must not be world-writable"
        );
    }
}

/// A Solana Rust job controls Cargo.toml, so code runs in the container: a program.so planted as a
/// symlink to a host file must fail the stage, and nothing of the target may be served.
#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_sandbox_output_fails_the_stage() {
    let secret = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(secret.path(), "host secret").unwrap();
    let target = secret.path().to_path_buf();
    let runner: Arc<dyn terminal_sandbox::Runner> = Arc::new(terminal_sandbox::FakeRunner::new(
        move |spec: &terminal_sandbox::RunSpec| {
            if spec.tool == terminal_sandbox::Tool::BuildSbf {
                std::os::unix::fs::symlink(&target, spec.work.join("program.so")).unwrap();
                return terminal_sandbox::RunOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                };
            }
            fake_answer(spec)
        },
    ));
    let (s, _d) = store_with(runner);
    let id = s.create(rust_job("pub fn x() {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    let e = v.compile.error.clone().unwrap_or_else(|| panic!("{v:?}"));
    assert_eq!(e.kind, "compile");
    assert!(e.message.contains("invalid output"), "{}", e.message);
    assert!(matches!(
        s.file(&id, "program.so"),
        Err(terminal_api::error::AppError::NotFound(_))
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_solc_output_fails_the_stage() {
    let secret = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(secret.path(), "{}").unwrap();
    let target = secret.path().to_path_buf();
    let runner: Arc<dyn terminal_sandbox::Runner> = Arc::new(terminal_sandbox::FakeRunner::new(
        move |spec: &terminal_sandbox::RunSpec| {
            if spec.tool == terminal_sandbox::Tool::Solc {
                std::os::unix::fs::symlink(&target, spec.work.join("output.json")).unwrap();
                return terminal_sandbox::RunOutput::default();
            }
            fake_answer(spec)
        },
    ));
    let (s, _d) = store_with(runner);
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert!(v.compile.error.unwrap().message.contains("invalid output"));
}

/// Links named like host artifacts inside sandbox/ are ignored: nothing is written through them.
#[cfg(unix)]
#[tokio::test]
async fn links_planted_in_the_sandbox_are_never_written_through() {
    let victim = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(victim.path(), "untouched").unwrap();
    let target = victim.path().to_path_buf();
    let runner: Arc<dyn terminal_sandbox::Runner> = Arc::new(terminal_sandbox::FakeRunner::new(
        move |spec: &terminal_sandbox::RunSpec| {
            let out = fake_answer(spec);
            for n in [
                "abi.json",
                "view.json",
                "view.tmp",
                "abi.tmp",
                "runtime.hex",
                "creation.hex",
                "job.json",
                "job.tmp",
                "source_map.json",
                "op_map.json",
                "contract.c",
            ] {
                let _ = std::os::unix::fs::symlink(&target, spec.work.join(n));
            }
            out
        },
    ));
    let (s, _d) = store_with(runner);
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    assert_eq!(std::fs::read_to_string(victim.path()).unwrap(), "untouched");
    let dir = s.dir(&id).unwrap();
    for host in [
        "compile/abi.json",
        "compile/view.json",
        "compile/runtime.hex",
        "build/view.json",
        "job.json",
    ] {
        assert!(
            std::fs::symlink_metadata(dir.join(host))
                .unwrap()
                .file_type()
                .is_file(),
            "{host}"
        );
    }
}

/// `file` and `stage_view` serve regular files only.
#[cfg(unix)]
#[tokio::test]
async fn served_files_must_be_regular() {
    let (s, _d) = store();
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    wait_finished(&s, &id).await;
    let dir = s.dir(&id).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    for rel in ["compile/abi.json", "compile/view.json"] {
        std::fs::remove_file(dir.join(rel)).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.join(rel)).unwrap();
    }
    assert!(s.file(&id, "abi.json").is_err());
    assert!(s.stage_view(&id, "compile").is_err());
}

#[tokio::test]
async fn sweep_leaves_running_jobs_alone() {
    let gated = GatedRunner::new();
    let (s, _d) = store_with(gated.clone());
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    while s.view(&id).unwrap().compile.status != StageStatus::Running {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        s.sweep(terminal_api::ratelimit::unix_now() + 3601),
        0,
        "a running job is never swept"
    );
    assert!(s.view(&id).is_ok());
    gated.open();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    assert_eq!(s.sweep(terminal_api::ratelimit::unix_now() + 3601), 1);
}

#[tokio::test]
async fn ids_are_validated() {
    let (s, _d) = store();
    assert!(s.dir("../etc").is_err());
    assert!(s.dir(&"0".repeat(32)).is_err(), "no such job");
    assert!(s.subscribe("../etc").is_none());
    for i in 0..100 {
        s.touch(&format!("junk{i}"));
    }
    assert_eq!(s.sweep(terminal_api::ratelimit::unix_now()), 0);
    assert_eq!(
        s.tracked(),
        0,
        "touching unknown ids does not grow the access map"
    );
}

async fn peak_with(max_builds: usize) -> usize {
    let counting = Arc::new(CountingRunner::default());
    let (s, _d) = store_cfg(counting.clone(), |c| c.max_builds = max_builds);
    let a = s.create(sol("contract ERC20 {}")).unwrap();
    let b = s.create(sol("contract ERC20 {}")).unwrap();
    for id in [a, b] {
        assert_eq!(wait_finished(&s, &id).await.build.status, StageStatus::Done);
    }
    assert_eq!(counting.calls.load(std::sync::atomic::Ordering::SeqCst), 6);
    counting.peak.load(std::sync::atomic::Ordering::SeqCst)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn max_builds_bounds_sandbox_concurrency() {
    assert_eq!(peak_with(1).await, 1);
    assert_eq!(
        peak_with(2).await,
        2,
        "the bound is the semaphore, not the test"
    );
}

#[tokio::test]
async fn a_reproducing_debug_build_gives_a_pc_source_map() {
    let runner: Arc<dyn terminal_sandbox::Runner> = Arc::new(terminal_sandbox::FakeRunner::new(
        |spec: &terminal_sandbox::RunSpec| {
            if spec.tool == (terminal_sandbox::Tool::RandGuestBuild { debug: true }) {
                std::fs::copy(spec.work.join("image.bin"), spec.work.join("image-dbg.bin"))
                    .unwrap();
                std::fs::write(spec.work.join("debug-line.txt"), "file_names[  0]:\n name: \"contract.c\"\nAddress Line Column File ISA Discriminator OpIndex Flags\n0x0000000000010000 7 5 0 0 0 0 is_stmt\n").unwrap();
                return terminal_sandbox::RunOutput::default();
            }
            fake_answer(spec)
        },
    ));
    let (s, _d) = store_with(runner);
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    let i: ImageView = serde_json::from_value(s.stage_view(&id, "image").unwrap()).unwrap();
    assert!(i.source_map_note.is_none(), "{:?}", i.source_map_note);
    assert_eq!(i.source_map.unwrap().pc_to_c, vec![(0x10000, 7)]);
    let dir = s.dir(&id).unwrap();
    assert!(
        dir.join("build/debug-line.txt").is_file() && dir.join("build/image-dbg.bin").is_file()
    );
}

/// C1: the container (another uid) leaves entries the API user cannot chmod. Those are skipped,
/// never an error, so the next sandbox step (the -g debug build) still runs.
#[cfg(unix)]
#[test]
fn make_world_writable_skips_entries_it_does_not_own() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let d = tempfile::tempdir().unwrap();
    let sub = d.path().join("target/release");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("prog"), "x").unwrap();
    std::fs::set_permissions(sub.join("prog"), std::fs::Permissions::from_mode(0o444)).unwrap();
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
    let me = std::fs::metadata(d.path()).unwrap().uid();
    // As if every entry belonged to the container's uid: nothing is changed, nothing fails.
    terminal_api::jobs::make_world_writable_as(d.path(), me.wrapping_add(1)).unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&sub), 0o555);
    assert_eq!(mode(&sub.join("prog")), 0o444);
    // As the owner: read-only entries (a file in a read-only dir) are opened up.
    terminal_api::jobs::make_world_writable(d.path()).unwrap();
    assert_eq!(mode(&sub), 0o777);
    assert_eq!(mode(&sub.join("prog")), 0o666);
}

/// C1: sweep deletes a job whose tree holds read-only directories (as a cargo target/ left by the
/// container can), and a removal error never stops the sweep.
#[cfg(unix)]
#[tokio::test]
async fn sweep_removes_job_dirs_with_read_only_nested_dirs() {
    use std::os::unix::fs::PermissionsExt;
    let runner: Arc<dyn terminal_sandbox::Runner> = Arc::new(terminal_sandbox::FakeRunner::new(
        |spec: &terminal_sandbox::RunSpec| {
            if spec.tool == (terminal_sandbox::Tool::RandGuestBuild { debug: true }) {
                let deps = spec.work.join("shim-dbg/target/release/deps");
                std::fs::create_dir_all(&deps).unwrap();
                std::fs::write(deps.join("libx.rlib"), "x").unwrap();
                for p in [deps.clone(), deps.parent().unwrap().to_path_buf()] {
                    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o555)).unwrap();
                }
            }
            fake_answer(spec)
        },
    ));
    let (s, _d) = store_with(runner);
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    assert_eq!(wait_finished(&s, &id).await.build.status, StageStatus::Done);
    let dir = s.dir(&id).unwrap();
    assert!(dir
        .join("build/sandbox/shim-dbg/target/release/deps/libx.rlib")
        .exists());
    assert_eq!(s.sweep(terminal_api::ratelimit::unix_now() + 3601), 1);
    assert!(!dir.exists(), "the whole job tree is gone");
}

/// I1: rand-guest writes image.bin and exits 1 when the checker rejects it (or it is over the
/// cap). The build is done and the image view reports fits:false; only a missing image fails.
#[tokio::test]
async fn a_rejected_image_is_built_and_shown_as_not_fitting() {
    let runner: Arc<dyn terminal_sandbox::Runner> = Arc::new(terminal_sandbox::FakeRunner::new(
        |spec: &terminal_sandbox::RunSpec| {
            let out = fake_answer(spec);
            if spec.tool == (terminal_sandbox::Tool::RandGuestBuild { debug: false }) {
                return terminal_sandbox::RunOutput {
                    status: 1,
                    stderr: "Error: the image is rejected by the checker (it was still written)"
                        .into(),
                    ..out
                };
            }
            out
        },
    ));
    let (s, _d) = store_cfg(runner, |c| c.max_program_words = 1000);
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    assert_eq!(v.build.status, StageStatus::Done, "{v:?}");
    let i: ImageView = serde_json::from_value(s.stage_view(&id, "image").unwrap()).unwrap();
    assert_eq!((i.words, i.cap_words, i.fits), (11_686, 1000, false));
    assert!(!i.findings.is_empty(), "the cap finding is listed");
}

#[tokio::test]
async fn a_build_without_an_image_fails() {
    let runner: Arc<dyn terminal_sandbox::Runner> = Arc::new(terminal_sandbox::FakeRunner::new(
        |spec: &terminal_sandbox::RunSpec| {
            if spec.tool == (terminal_sandbox::Tool::RandGuestBuild { debug: false }) {
                return terminal_sandbox::RunOutput {
                    status: 101,
                    stderr: "error: cargo build failed".into(),
                    ..Default::default()
                };
            }
            fake_answer(spec)
        },
    ));
    let (s, _d) = store_with(runner);
    let id = s.create(sol("contract ERC20 {}")).unwrap();
    let v = wait_finished(&s, &id).await;
    let e = v.build.error.clone().unwrap_or_else(|| panic!("{v:?}"));
    assert_eq!(e.kind, "build");
    assert!(e.message.contains("exited 101"), "{}", e.message);
}
