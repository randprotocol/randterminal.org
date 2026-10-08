use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use terminal_sandbox::*;

#[test]
fn docker_arguments_are_the_locked_down_set() {
    let r = DockerRunner::new("terminal-sandbox:test");
    let spec = RunSpec { tool: Tool::RandGuestBuild { debug: true }, work: "/data/jobs/abc/build".into(), timeout: Duration::from_secs(5) };
    let a = r.args(&spec, "terminal-abc");
    let joined = a.join(" ");
    for want in ["run", "--rm", "--name terminal-abc", "--network none", "--memory 4g", "--memory-swap 4g", "--cpus 2", "--pids-limit 512",
                 "--read-only", "--tmpfs /tmp:rw,exec,size=2g", "--security-opt no-new-privileges", "--cap-drop ALL", "--user 10001:10001",
                 "-v /data/jobs/abc/build:/circuits/jobs/job:rw", "terminal-sandbox:test", "rand-guest-build debug"] {
        assert!(joined.contains(want), "missing `{want}` in `{joined}`");
    }
    assert!(joined.ends_with("terminal-sandbox:test rand-guest-build debug"));
    let spec = RunSpec { tool: Tool::Solc, work: "/w".into(), timeout: Duration::from_secs(5) };
    assert!(r.args(&spec, "n").join(" ").contains("-v /w:/work:rw"));
    assert!(r.args(&spec, "n").join(" ").ends_with(" solc"));
}

#[test]
fn timeouts() {
    assert_eq!(timeout_for(&Tool::Solc), Duration::from_secs(30));
    assert_eq!(timeout_for(&Tool::BuildSbf), Duration::from_secs(300));
    assert_eq!(timeout_for(&Tool::RandGuestBuild { debug: false }), Duration::from_secs(180));
}

/// A stand-in for `docker`: prints a lot, or sleeps, depending on the last argument.
fn fake_docker(dir: &std::path::Path) -> String {
    let p = dir.join("fake-docker");
    std::fs::write(&p, "#!/bin/sh\ncase \"$1\" in kill) exit 0;; esac\nlast=\"\"; for a in \"$@\"; do last=\"$a\"; done\ncase \"$last\" in\n  solc) head -c 3000000 /dev/zero | tr '\\0' 'x'; echo err >&2; exit 3;;\n  build-sbf) sleep 30;;\nesac\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

#[tokio::test]
async fn captures_are_capped_and_status_is_returned() {
    let d = tempfile::tempdir().unwrap();
    let mut r = DockerRunner::new("img");
    r.program = fake_docker(d.path());
    let out = r.run(RunSpec { tool: Tool::Solc, work: d.path().into(), timeout: Duration::from_secs(10) }).await.unwrap();
    assert_eq!(out.status, 3);
    assert!(!out.timed_out);
    assert_eq!(out.stdout.len(), MAX_CAPTURE);
    assert_eq!(out.stderr.trim(), "err");
}

#[tokio::test]
async fn timeouts_kill_and_report() {
    let d = tempfile::tempdir().unwrap();
    let mut r = DockerRunner::new("img");
    r.program = fake_docker(d.path());
    let t = std::time::Instant::now();
    let out = r.run(RunSpec { tool: Tool::BuildSbf, work: d.path().into(), timeout: Duration::from_millis(300) }).await.unwrap();
    assert!(out.timed_out);
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn fake_runner_calls_the_closure() {
    let r = FakeRunner::new(|spec: &RunSpec| { std::fs::write(spec.work.join("out.txt"), "hi").unwrap(); RunOutput { status: 0, stdout: "ok".into(), stderr: String::new(), timed_out: false } });
    let d = tempfile::tempdir().unwrap();
    let out = r.run(RunSpec { tool: Tool::Solc, work: d.path().into(), timeout: Duration::from_secs(1) }).await.unwrap();
    assert_eq!(out.stdout, "ok");
    assert_eq!(std::fs::read_to_string(d.path().join("out.txt")).unwrap(), "hi");
}
