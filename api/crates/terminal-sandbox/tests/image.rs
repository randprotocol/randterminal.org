//! Real builds in the real image. Skips unless TERMINAL_SANDBOX names the image tag.
use terminal_core::{evm_analysis, sbpf_analysis, stages};
use terminal_sandbox::*;

fn image() -> Option<DockerRunner> {
    match std::env::var("TERMINAL_SANDBOX") {
        Ok(t) => Some(DockerRunner::new(t)),
        Err(_) => {
            eprintln!("skipping: TERMINAL_SANDBOX unset");
            None
        }
    }
}
fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../terminal-core/tests/fixtures")
}
fn info_hc(name: &str) -> String {
    std::fs::read_to_string(fixtures().join(name))
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("hc ").map(str::to_string))
        .unwrap()
}
fn spec(tool: Tool, work: &std::path::Path) -> RunSpec {
    RunSpec {
        tool,
        work: work.into(),
        timeout: timeout_for(&tool),
    }
}
/// A scratch directory the container's uid 10001 can write.
fn work() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    d
}

/// Makes everything written so far (shim/ included) writable by uid 10001, which creates shim/target.
fn open_up(dir: &std::path::Path) {
    let st = std::process::Command::new("chmod")
        .arg("-R")
        .arg("a+rwX")
        .arg(dir)
        .status()
        .unwrap();
    assert!(st.success());
}

#[tokio::test]
async fn erc20_sol_compiles_to_the_fixture_bytecode() {
    let Some(r) = image() else { return };
    let d = work();
    let src = std::fs::read_to_string(fixtures().join("ERC20.sol")).unwrap();
    let files = vec![("ERC20.sol".to_string(), src)];
    std::fs::write(
        d.path().join("input.json"),
        stages::solc_input(&files).to_string(),
    )
    .unwrap();
    open_up(d.path());
    let out = r.run(spec(Tool::Solc, d.path())).await.unwrap();
    assert_eq!(out.status, 0, "{}", out.stderr);
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(d.path().join("output.json")).unwrap()).unwrap();
    let res = stages::parse_solc_output(&v, &files, Some("ERC20")).unwrap();
    let want = std::fs::read_to_string(fixtures().join("erc20.runtime.hex")).unwrap();
    assert_eq!(
        hex::encode(&res.runtime),
        want.trim().trim_start_matches("0x")
    );
}

#[tokio::test]
async fn erc20_shim_builds_to_the_published_hc_and_the_debug_map_lines_up() {
    let Some(r) = image() else { return };
    let d = work();
    let code = hex::decode(
        std::fs::read_to_string(fixtures().join("erc20.runtime.hex"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    let (view, c) = evm_analysis::transpile(&code, 13, 2).unwrap();
    stages::write_evm_shim(&d.path().join("shim"), "contract", &c).unwrap();
    open_up(d.path());
    let out = r
        .run(spec(Tool::RandGuestBuild { debug: false }, d.path()))
        .await
        .unwrap();
    assert_eq!(out.status, 0, "{}\n{}", out.stdout, out.stderr);
    let img = std::fs::read(d.path().join("image.bin")).unwrap();
    let iv = terminal_core::image::view(&img, 65_535, &view.op_map, None, "contract.c").unwrap();
    assert_eq!(iv.hc, info_hc("erc20.info.txt"));
    let out = r
        .run(spec(Tool::RandGuestBuild { debug: true }, d.path()))
        .await
        .unwrap();
    assert_eq!(out.status, 0, "{}\n{}", out.stdout, out.stderr);
    let dbg = std::fs::read(d.path().join("image-dbg.bin")).unwrap();
    let (_, t1, _) = rand_guest::pack::split(&img).unwrap();
    let (_, t2, _) = rand_guest::pack::split(&dbg).unwrap();
    assert_eq!(
        t1, t2,
        "-g changed the code: the debug map would not describe the production image"
    );
    let dl = std::fs::read_to_string(d.path().join("debug-line.txt")).unwrap();
    let rows = terminal_core::source_map::parse_debug_line(&dl, "contract.c");
    assert!(rows.len() > 500, "{} rows", rows.len());
}

#[tokio::test]
async fn spl_shim_builds_to_the_published_hc() {
    let Some(r) = image() else { return };
    let d = work();
    let a =
        sbpf_analysis::analyse(&std::fs::read(fixtures().join("spl_token.so")).unwrap()).unwrap();
    stages::write_sbpf_shim(&d.path().join("shim"), "spl-token", &a.program_c).unwrap();
    open_up(d.path());
    let out = r
        .run(spec(Tool::RandGuestBuild { debug: false }, d.path()))
        .await
        .unwrap();
    assert_eq!(out.status, 0, "{}\n{}", out.stdout, out.stderr);
    let iv = terminal_core::image::view(
        &std::fs::read(d.path().join("image.bin")).unwrap(),
        65_535,
        &[],
        None,
        "program.c",
    )
    .unwrap();
    assert_eq!(iv.hc, info_hc("spl.info.txt"));
}

#[tokio::test]
async fn solana_counter_builds_from_source_and_transpiles() {
    let Some(r) = image() else { return };
    let d = work();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    let tpl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../sandbox/solana-template");
    std::fs::copy(tpl.join("src/lib.rs"), d.path().join("src/lib.rs")).unwrap();
    open_up(d.path());
    let out = r.run(spec(Tool::BuildSbf, d.path())).await.unwrap();
    assert_eq!(out.status, 0, "{}\n{}", out.stdout, out.stderr);
    let a = sbpf_analysis::analyse(&std::fs::read(d.path().join("program.so")).unwrap()).unwrap();
    assert!(a.compile.syscalls.iter().any(|s| s.name == "sol_log_"));
}

#[tokio::test]
async fn a_rust_error_comes_back_as_a_diagnostic() {
    let Some(r) = image() else { return };
    let d = work();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    std::fs::write(d.path().join("src/lib.rs"), "pub fn f() -> u8 { y }\n").unwrap();
    open_up(d.path());
    let out = r.run(spec(Tool::BuildSbf, d.path())).await.unwrap();
    assert_ne!(out.status, 0);
    let diags = stages::rustc_diagnostics(&out.stdout);
    assert!(
        diags.iter().any(|d| d.severity == "error" && d.line == 1),
        "{diags:?}\n{}",
        out.stderr
    );
}
