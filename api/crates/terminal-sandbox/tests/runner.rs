use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use terminal_sandbox::*;

#[test]
fn docker_arguments_are_the_locked_down_set() {
    let r = DockerRunner::new("terminal-sandbox:test");
    let spec = RunSpec {
        tool: Tool::RandGuestBuild { debug: true },
        work: "/data/jobs/abc/build".into(),
        timeout: Duration::from_secs(5),
    };
    let a = r.args(&spec, "terminal-abc");
    let joined = a.join(" ");
    for want in [
        "run",
        "--rm",
        "--name terminal-abc",
        "--network none",
        "--memory 4g",
        "--memory-swap 4g",
        "--cpus 2",
        "--pids-limit 512",
        "--read-only",
        "--tmpfs /tmp:rw,exec,size=2g",
        "--security-opt no-new-privileges",
        "--cap-drop ALL",
        "--user 10001:10001",
        "-v /data/jobs/abc/build:/circuits/jobs/job:rw",
        "terminal-sandbox:test",
        "rand-guest-build debug",
    ] {
        assert!(joined.contains(want), "missing `{want}` in `{joined}`");
    }
    assert!(joined.ends_with("terminal-sandbox:test rand-guest-build debug"));
    let spec = RunSpec {
        tool: Tool::Solc,
        work: "/w".into(),
        timeout: Duration::from_secs(5),
    };
    assert!(r.args(&spec, "n").join(" ").contains("-v /w:/work:rw"));
    assert!(r.args(&spec, "n").join(" ").ends_with(" solc"));
}

#[test]
fn timeouts() {
    assert_eq!(timeout_for(&Tool::Solc), Duration::from_secs(30));
    assert_eq!(timeout_for(&Tool::BuildSbf), Duration::from_secs(300));
    assert_eq!(
        timeout_for(&Tool::RandGuestBuild { debug: false }),
        Duration::from_secs(180)
    );
}

/// A stand-in for `docker`: prints a lot, or sleeps, depending on the last argument.
fn fake_docker(dir: &std::path::Path) -> String {
    let p = dir.join("fake-docker");
    std::fs::write(&p, "#!/bin/sh\necho \"$@\" >> \"$(dirname \"$0\")/log\"\ncase \"$1\" in kill) exit 0;; esac\nlast=\"\"; for a in \"$@\"; do last=\"$a\"; done\ncase \"$last\" in\n  solc) head -c 3000000 /dev/zero | tr '\\0' 'x'; echo err >&2; exit 3;;\n  build-sbf) sleep 30;;\nesac\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

#[tokio::test]
async fn captures_are_capped_and_status_is_returned() {
    let d = tempfile::tempdir().unwrap();
    let mut r = DockerRunner::new("img");
    r.program = fake_docker(d.path());
    let out = r
        .run(RunSpec {
            tool: Tool::Solc,
            work: d.path().into(),
            timeout: Duration::from_secs(10),
        })
        .await
        .unwrap();
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
    let out = r
        .run(RunSpec {
            tool: Tool::BuildSbf,
            work: d.path().into(),
            timeout: Duration::from_millis(300),
        })
        .await
        .unwrap();
    assert!(out.timed_out);
    assert!(t.elapsed() < Duration::from_secs(5));
    let log = std::fs::read_to_string(d.path().join("log")).unwrap();
    let name = container_name(&log);
    assert!(
        log.lines().any(|l| l == format!("kill {name}")),
        "no kill of {name} in:\n{log}"
    );
}

fn container_name(log: &str) -> String {
    let run = log
        .lines()
        .find(|l| l.starts_with("run "))
        .expect("run line");
    let mut it = run.split(' ');
    while let Some(w) = it.next() {
        if w == "--name" {
            return it.next().unwrap().to_string();
        }
    }
    panic!("no --name in {run}");
}

#[tokio::test]
async fn a_dropped_run_kills_its_container() {
    let d = tempfile::tempdir().unwrap();
    let mut r = DockerRunner::new("img");
    r.program = fake_docker(d.path());
    let fut = r.run(RunSpec {
        tool: Tool::BuildSbf,
        work: d.path().into(),
        timeout: Duration::from_secs(30),
    });
    assert!(tokio::time::timeout(Duration::from_millis(300), fut)
        .await
        .is_err());
    // The container name is generated inside `run`, so look for any `kill terminal-*` line; the
    // stand-in may not have logged its `run` line yet under load.
    let log_path = d.path().join("log");
    for _ in 0..40 {
        let log = std::fs::read_to_string(&log_path).unwrap_or_default();
        if log.lines().any(|l| l.starts_with("kill terminal-")) {
            if log.lines().any(|l| l.starts_with("run ")) {
                let name = container_name(&log);
                assert!(log.lines().any(|l| l == format!("kill {name}")), "{log}");
            }
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "no kill after drop: {}",
        std::fs::read_to_string(&log_path).unwrap_or_default()
    );
}

#[tokio::test]
async fn unsafe_work_paths_are_rejected() {
    let r = DockerRunner::new("img");
    for w in ["relative/dir", "/a:b", "/a,b", "/a\nb"] {
        let e = r
            .run(RunSpec {
                tool: Tool::Solc,
                work: w.into(),
                timeout: Duration::from_secs(1),
            })
            .await
            .unwrap_err();
        assert!(matches!(e, SandboxError::Spawn(_)), "{w}");
    }
}

#[tokio::test]
async fn invalid_utf8_capture_never_exceeds_the_cap() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("fake-docker");
    // 0xff becomes a 3-byte U+FFFD after lossy conversion.
    std::fs::write(
        &p,
        "#!/bin/sh\nhead -c 3000000 /dev/zero | LC_ALL=C tr '\\0' '\\377'\n",
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut r = DockerRunner::new("img");
    r.program = p.to_string_lossy().into_owned();
    let out = r
        .run(RunSpec {
            tool: Tool::Solc,
            work: d.path().into(),
            timeout: Duration::from_secs(10),
        })
        .await
        .unwrap();
    assert!(out.stdout.len() <= MAX_CAPTURE);
    assert!(out.stdout.len() > MAX_CAPTURE - 3);
    assert!(out.stdout.chars().all(|c| c == '\u{fffd}'));
}

#[tokio::test]
async fn fake_runner_calls_the_closure() {
    let r = FakeRunner::new(|spec: &RunSpec| {
        std::fs::write(spec.work.join("out.txt"), "hi").unwrap();
        RunOutput {
            status: 0,
            stdout: "ok".into(),
            stderr: String::new(),
            timed_out: false,
        }
    });
    let d = tempfile::tempdir().unwrap();
    let out = r
        .run(RunSpec {
            tool: Tool::Solc,
            work: d.path().into(),
            timeout: Duration::from_secs(1),
        })
        .await
        .unwrap();
    assert_eq!(out.stdout, "ok");
    assert_eq!(
        std::fs::read_to_string(d.path().join("out.txt")).unwrap(),
        "hi"
    );
}

#[test]
fn docker_failures_are_told_apart_from_tool_failures() {
    use terminal_sandbox::{docker_unavailable, RunOutput};
    let out = |status, stderr: &str| RunOutput {
        status,
        stderr: stderr.into(),
        ..Default::default()
    };
    assert!(docker_unavailable(&out(
        125,
        "docker: Error response from daemon: No such image: x"
    )));
    assert!(docker_unavailable(&out(
        1,
        "docker: Cannot connect to the Docker daemon at unix:///var/run/docker.sock."
    )));
    assert!(docker_unavailable(&out(
        125,
        "docker: failed to connect to the docker API at unix:///x"
    )));
    assert!(
        !docker_unavailable(&out(125, "error[E0425]: cannot find value")),
        "a tool's own 125"
    );
    assert!(!docker_unavailable(&out(
        1,
        "Error: the image is rejected by the checker"
    )));
}

/// M3: only lines that begin with `docker:` count, so a compiler's own output cannot read as
/// "the sandbox is unavailable"; docker's prefix lines (a pull attempt) do not hide its error.
#[test]
fn only_docker_prefixed_lines_say_the_sandbox_is_down() {
    use terminal_sandbox::{docker_unavailable, RunOutput};
    let out = |status, stderr: &str| RunOutput {
        status,
        stderr: stderr.into(),
        ..Default::default()
    };
    assert!(docker_unavailable(&out(
        125,
        "Unable to find image 'terminal-sandbox:x' locally\ndocker: Error response from daemon: pull access denied for terminal-sandbox"
    )));
    assert!(docker_unavailable(&out(
        125,
        "warning: something\n  docker: failed to connect to the docker API at unix:///x\n"
    )));
    // User output: a mention mid-line, a daemon message without docker's prefix, a 125 whose
    // docker: line is not the start of a line.
    assert!(!docker_unavailable(&out(
        1,
        "error: Cannot connect to the Docker daemon (user #error text)"
    )));
    assert!(!docker_unavailable(&out(
        1,
        "Cannot connect to the Docker daemon at unix:///x"
    )));
    assert!(!docker_unavailable(&out(
        125,
        "error: see docker: failed to connect to the docker API"
    )));
    assert!(!docker_unavailable(&out(
        1,
        "error[E0425]: cannot find value\n#error docker: failed to connect to the docker API"
    )));
}

#[tokio::test]
async fn availability_is_the_docker_version_probe() {
    use terminal_sandbox::Runner;
    let mut r = DockerRunner::new("x");
    r.program = "false".into();
    assert!(!r.available().await);
    r.program = "true".into();
    assert!(r.available().await);
    r.program = "/nonexistent/docker".into();
    assert!(!r.available().await);
}
