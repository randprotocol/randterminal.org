//! Jobs on disk and the pipeline that fills them. One directory per job (layout at the top of the
//! plan); `job.json` is rewritten after every change and every change is broadcast for SSE.
//!
//! The sandbox is untrusted: a Solana Rust job controls its Cargo.toml, so build scripts run in the
//! container. Each step therefore mounts only a dedicated `<stage>/sandbox/` directory, and the
//! host takes back exactly the outputs it expects from there (regular files only, opened without
//! following links, size-capped) into host-owned paths the container never sees.
use crate::error::{ApiResult, AppError};
use crate::ratelimit::unix_now;
use crate::ApiConfig;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use terminal_core::views::*;
use terminal_core::{evm_analysis, image, sbpf_analysis, stages};
use terminal_sandbox::{timeout_for, RunOutput, RunSpec, Runner, Tool};
use tokio::sync::{broadcast, Semaphore};

pub const MAX_TEXT_BYTES: usize = 512 * 1024;
pub const MAX_ELF_BYTES: usize = 2 * 1024 * 1024;
const CONSOLE_KEEP: usize = 64 * 1024;

/// Size caps on what the host takes back from a sandbox step.
const MAX_SOLC_OUTPUT: u64 = 32 << 20;
const MAX_IMAGE: u64 = 1 << 20;
const MAX_DEBUG_LINE: u64 = 64 << 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Stage { stage: StageName, state: StageState },
    Console(ConsoleLine),
    Done,
}

pub struct JobStore {
    root: PathBuf,
    config: Arc<ApiConfig>,
    runner: Arc<dyn Runner>,
    builds: Arc<Semaphore>,
    chans: Mutex<HashMap<String, broadcast::Sender<JobEvent>>>,
    access: Mutex<HashMap<String, u64>>,
    running: Mutex<HashSet<String>>,
}

fn queued() -> StageState {
    StageState {
        status: StageStatus::Queued,
        started: None,
        ended: None,
        error: None,
    }
}
fn stage_err(kind: &str, message: impl Into<String>) -> StageError {
    StageError {
        kind: kind.into(),
        message: message.into(),
        diagnostics: vec![],
    }
}
fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 64
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        && !n.starts_with('.')
}
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())
}
fn tail(s: &str) -> String {
    if s.len() <= CONSOLE_KEEP {
        s.to_string()
    } else {
        let mut i = s.len() - CONSOLE_KEEP;
        while !s.is_char_boundary(i) {
            i += 1;
        }
        s[i..].to_string()
    }
}
fn is_regular(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_file())
        .unwrap_or(false)
}

/// Reads a host-owned file, refusing anything that is not a regular file (never a link).
pub fn read_regular(p: &Path) -> std::io::Result<Vec<u8>> {
    if !is_regular(p) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is not a regular file", p.display()),
        ));
    }
    std::fs::read(p)
}

impl JobStore {
    pub fn new(config: Arc<ApiConfig>, runner: Arc<dyn Runner>) -> Arc<Self> {
        // The docker runner only mounts absolute paths, so resolve the data dir once here.
        let _ = std::fs::create_dir_all(config.data_dir.join("jobs"));
        let root = std::fs::canonicalize(&config.data_dir)
            .or_else(|_| std::env::current_dir().map(|c| c.join(&config.data_dir)))
            .unwrap_or_else(|_| config.data_dir.clone())
            .join("jobs");
        mark_interrupted(&root);
        Arc::new(Self {
            root,
            builds: Arc::new(Semaphore::new(config.max_builds.max(1))),
            config,
            runner,
            chans: Mutex::new(HashMap::new()),
            access: Mutex::new(HashMap::new()),
            running: Mutex::new(HashSet::new()),
        })
    }

    /// The directory of an existing job (validated id).
    pub fn dir(&self, id: &str) -> ApiResult<PathBuf> {
        self.check_id(id)
    }
    fn path(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn check_id(&self, id: &str) -> ApiResult<PathBuf> {
        if !valid_id(id) {
            return Err(AppError::NotFound(format!("job {id}")));
        }
        let d = self.path(id);
        if !is_regular(&d.join("job.json")) {
            return Err(AppError::NotFound(format!("job {id}")));
        }
        Ok(d)
    }

    /// Records an access to an existing job; unknown ids are ignored.
    pub fn touch(&self, id: &str) {
        if self.check_id(id).is_ok() {
            self.access
                .lock()
                .unwrap()
                .insert(id.to_string(), unix_now());
        }
    }

    /// How many jobs have a recorded access time.
    pub fn tracked(&self) -> usize {
        self.access.lock().unwrap().len()
    }

    pub fn create(self: &Arc<Self>, req: NewJob) -> ApiResult<String> {
        if !(1..=2).contains(&req.options.stage) {
            return Err(AppError::BadRequest(format!(
                "stage must be 1 or 2, not {}",
                req.options.stage
            )));
        }
        let mut text_total = 0usize;
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        let mut seen = HashSet::new();
        for f in &req.files {
            if !valid_name(&f.name) {
                return Err(AppError::BadRequest(format!("bad file name {:?}", f.name)));
            }
            if !seen.insert(f.name.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "duplicate file name {:?}",
                    f.name
                )));
            }
            let bytes = match f.encoding.as_str() {
                "text" => {
                    text_total += f.content.len();
                    f.content.clone().into_bytes()
                }
                "base64" => base64::engine::general_purpose::STANDARD
                    .decode(&f.content)
                    .map_err(|e| AppError::BadRequest(format!("{}: {e}", f.name)))?,
                e => return Err(AppError::BadRequest(format!("encoding {e:?}"))),
            };
            files.push((f.name.clone(), bytes));
        }
        if text_total > MAX_TEXT_BYTES {
            return Err(AppError::PayloadTooLarge(format!(
                "{text_total} bytes of source; the limit is {MAX_TEXT_BYTES}"
            )));
        }
        let names: Vec<&str> = req.files.iter().map(|f| f.name.as_str()).collect();
        match req.lang {
            Lang::Solidity => {
                if files.is_empty() || files.len() > 16 {
                    return Err(AppError::BadRequest(
                        "a Solidity job has 1 to 16 .sol files".into(),
                    ));
                }
                if let Some(f) = req
                    .files
                    .iter()
                    .find(|f| !f.name.ends_with(".sol") || f.encoding != "text")
                {
                    return Err(AppError::BadRequest(format!(
                        "{}: Solidity jobs take .sol text files",
                        f.name
                    )));
                }
            }
            Lang::SolanaRust => {
                if !names.contains(&"lib.rs")
                    || names.iter().any(|n| *n != "lib.rs" && *n != "Cargo.toml")
                    || req.files.iter().any(|f| f.encoding != "text")
                {
                    return Err(AppError::BadRequest(
                        "a Solana Rust job is lib.rs and optionally Cargo.toml, as text".into(),
                    ));
                }
            }
            Lang::SolanaElf => {
                if names != ["program.so"] || req.files[0].encoding != "base64" {
                    return Err(AppError::BadRequest(
                        "a Solana ELF job is one base64 file named program.so".into(),
                    ));
                }
                if files[0].1.len() > MAX_ELF_BYTES {
                    return Err(AppError::PayloadTooLarge(format!(
                        "{} bytes of ELF; the limit is {MAX_ELF_BYTES}",
                        files[0].1.len()
                    )));
                }
            }
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = self.path(&id);
        // Marked running before its directory exists, so a concurrent sweep never sees it unmarked.
        self.running.lock().unwrap().insert(id.clone());
        if let Err(e) = std::fs::create_dir_all(dir.join("source")) {
            self.running.lock().unwrap().remove(&id);
            return Err(e.into());
        }
        let view = JobView {
            id: id.clone(),
            lang: req.lang,
            options: req.options,
            created: unix_now(),
            compile: queued(),
            transpile: queued(),
            build: queued(),
            console: vec![],
            finished: false,
        };
        let written = files
            .iter()
            .try_for_each(|(n, b)| std::fs::write(dir.join("source").join(n), b))
            .and_then(|_| write_json(&dir.join("job.json"), &view));
        if let Err(e) = written {
            self.running.lock().unwrap().remove(&id);
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e.into());
        }
        let (tx, _) = broadcast::channel(256);
        self.chans.lock().unwrap().insert(id.clone(), tx);
        self.touch(&id);
        let me = self.clone();
        tokio::spawn(async move {
            Pipeline {
                store: me,
                dir,
                view,
                completed: false,
            }
            .run()
            .await
        });
        Ok(id)
    }

    pub fn view(&self, id: &str) -> ApiResult<JobView> {
        let d = self.check_id(id)?;
        self.touch(id);
        Ok(serde_json::from_slice(&std::fs::read(d.join("job.json"))?)?)
    }

    pub fn stage_view(&self, id: &str, stage: &str) -> ApiResult<serde_json::Value> {
        let d = self.check_id(id)?;
        let sub = match stage {
            "compile" => "compile",
            "transpile" => "transpile",
            "image" => "build",
            _ => return Err(AppError::NotFound(stage.into())),
        };
        let p = d.join(sub).join("view.json");
        if !is_regular(&p) {
            return Err(AppError::Conflict(format!("the {stage} view is not ready")));
        }
        self.touch(id);
        Ok(serde_json::from_slice(&std::fs::read(p)?)?)
    }

    pub fn file(&self, id: &str, name: &str) -> ApiResult<(Vec<u8>, &'static str)> {
        let d = self.check_id(id)?;
        let (rel, ty) = match name {
            "contract.c" => ("transpile/contract.c", "text/plain; charset=utf-8"),
            "program.c" => ("transpile/program.c", "text/plain; charset=utf-8"),
            "runtime.hex" => ("compile/runtime.hex", "text/plain; charset=utf-8"),
            "abi.json" => ("compile/abi.json", "application/json"),
            "program.so" => ("compile/program.so", "application/octet-stream"),
            "image.bin" => ("build/image.bin", "application/octet-stream"),
            _ => return Err(AppError::NotFound(name.into())),
        };
        let p = d.join(rel);
        if !is_regular(&p) {
            return Err(AppError::NotFound(name.into()));
        }
        Ok((std::fs::read(p)?, ty))
    }

    pub fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<JobEvent>> {
        if !valid_id(id) {
            return None;
        }
        self.chans.lock().unwrap().get(id).map(|t| t.subscribe())
    }

    fn last_access(&self, id: &str) -> u64 {
        if let Some(t) = self.access.lock().unwrap().get(id) {
            return *t;
        }
        std::fs::symlink_metadata(self.path(id).join("job.json"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Deletes a job's tree; false (logged) when some of it could not be deleted. A read-only
    /// directory this process owns is opened up and the delete retried once; what the container
    /// left that we can neither change nor delete stays for a later sweep.
    fn remove(&self, id: &str) -> bool {
        let p = self.path(id);
        let mut r = std::fs::remove_dir_all(&p);
        if r.is_err() && p.exists() {
            let _ = make_world_writable(&p);
            r = std::fs::remove_dir_all(&p);
        }
        self.access.lock().unwrap().remove(id);
        self.chans.lock().unwrap().remove(id);
        match r {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => {
                tracing::warn!("sweep: could not delete all of job {id}: {e}");
                false
            }
        }
    }

    /// Removes expired jobs, then least recently used ones while over the disk cap. Jobs whose
    /// pipeline is still running are never removed. Blocking: walks the job tree.
    pub fn sweep(&self, now: u64) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let mut jobs: Vec<(String, u64, u64)> = Vec::new();
        let mut running_size = 0;
        for e in rd.filter_map(|e| e.ok()) {
            let id = e.file_name().to_string_lossy().into_owned();
            let size = dir_size(&e.path());
            if self.running.lock().unwrap().contains(&id) {
                running_size += size;
                continue;
            }
            jobs.push((id.clone(), self.last_access(&id), size));
        }
        let mut removed = 0;
        jobs.retain(|(id, at, _)| {
            if now.saturating_sub(*at) > self.config.job_ttl_secs {
                self.remove(id);
                removed += 1;
                false
            } else {
                true
            }
        });
        let mut total: u64 = running_size + jobs.iter().map(|j| j.2).sum::<u64>();
        jobs.sort_by_key(|j| j.1);
        for (id, _, size) in jobs {
            if total <= self.config.data_cap_bytes {
                break;
            }
            self.remove(&id);
            removed += 1;
            total -= size;
        }
        removed
    }

    pub fn spawn_sweeper(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                let me = self.clone();
                match tokio::task::spawn_blocking(move || me.sweep(unix_now())).await {
                    Ok(n) if n > 0 => tracing::info!("swept {n} job(s)"),
                    Ok(_) => {}
                    Err(e) => tracing::error!("sweep: {e}"),
                }
            }
        });
    }
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| {
                    let m = e.metadata().ok();
                    if m.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
                        dir_size(&e.path())
                    } else {
                        m.map(|m| m.len()).unwrap_or(0)
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Writes `v` as JSON through a fresh temporary file (created with O_EXCL, so never through a
/// pre-existing link) renamed over `p`.
/// Jobs a previous process left unfinished can never finish: fail them so clients see an end.
fn mark_interrupted(root: &Path) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path().join("job.json");
        if !is_regular(&p) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        let Ok(mut view) = serde_json::from_slice::<JobView>(&bytes) else {
            continue;
        };
        if view.finished {
            continue;
        }
        fail_interrupted(&mut view, "interrupted by a restart");
        let _ = write_json(&p, &view);
    }
}

pub fn write_json<T: Serialize>(p: &Path, v: &T) -> std::io::Result<()> {
    let tmp = p.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    f.write_all(&serde_json::to_vec(v)?)?;
    drop(f);
    std::fs::rename(tmp, p)
}

#[cfg(unix)]
fn open_nofollow(p: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    // O_NONBLOCK so a planted FIFO cannot hang the open; it has no effect on regular files.
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(p)
}

#[cfg(not(unix))]
fn open_nofollow(p: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(p)
}

/// Takes back an output the sandbox step was asked to produce: `Ok(None)` when it is absent, a
/// stage error of `kind` when it is anything but a regular file of at most `cap` bytes.
fn take_output(
    sandbox: &Path,
    name: &str,
    cap: u64,
    kind: &str,
) -> Result<Option<Vec<u8>>, StageError> {
    let p = sandbox.join(name);
    let invalid = |why: String| {
        stage_err(
            kind,
            format!("the sandbox produced an invalid output {name}: {why}"),
        )
    };
    match std::fs::symlink_metadata(&p) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => return Err(invalid("not a regular file".into())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(e)),
    }
    let f = open_nofollow(&p).map_err(|e| invalid(e.to_string()))?;
    let m = f.metadata().map_err(io_err)?;
    if !m.is_file() {
        return Err(invalid("not a regular file".into()));
    }
    if m.len() > cap {
        return Err(invalid(format!("{} bytes; the limit is {cap}", m.len())));
    }
    let mut buf = Vec::new();
    f.take(cap + 1).read_to_end(&mut buf).map_err(io_err)?;
    if buf.len() as u64 > cap {
        return Err(invalid(format!("more than {cap} bytes")));
    }
    Ok(Some(buf))
}

struct Pipeline {
    store: Arc<JobStore>,
    dir: PathBuf,
    view: JobView,
    completed: bool,
}

/// Fails the stage that was running (else the first one still queued) and finishes the job.
fn fail_interrupted(view: &mut JobView, message: &str) {
    let now = unix_now();
    let st = if view.compile.status == StageStatus::Running {
        &mut view.compile
    } else if view.transpile.status == StageStatus::Running {
        &mut view.transpile
    } else if view.build.status == StageStatus::Running {
        &mut view.build
    } else if view.compile.status == StageStatus::Queued {
        &mut view.compile
    } else if view.transpile.status == StageStatus::Queued {
        &mut view.transpile
    } else {
        &mut view.build
    };
    st.status = StageStatus::Failed;
    st.ended = Some(now);
    st.error = Some(stage_err("internal", message));
    view.finished = true;
}

impl Drop for Pipeline {
    /// A pipeline that panicked or was dropped before `run` finished still ends its job.
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        fail_interrupted(&mut self.view, "the pipeline stopped unexpectedly");
        self.save();
        self.store.running.lock().unwrap().remove(&self.view.id);
        let snapshot = self.view.clone();
        for (stage, st) in [
            (StageName::Compile, &snapshot.compile),
            (StageName::Transpile, &snapshot.transpile),
            (StageName::Build, &snapshot.build),
        ] {
            if st.status == StageStatus::Failed {
                self.emit(JobEvent::Stage {
                    stage,
                    state: st.clone(),
                });
            }
        }
        self.emit(JobEvent::Done);
    }
}

/// What the compile stage hands the transpile stage.
enum Compiled {
    Evm(Vec<u8>),
    Sbpf(Box<sbpf_analysis::Analysis>),
}
/// What the transpile stage hands the build stage.
struct Transpiled {
    c_file: &'static str,
    op_map: Vec<OpLine>,
}

impl Pipeline {
    fn emit(&self, e: JobEvent) {
        if let Some(tx) = self.store.chans.lock().unwrap().get(&self.view.id) {
            let _ = tx.send(e);
        }
    }
    fn save(&self) {
        let _ = write_json(&self.dir.join("job.json"), &self.view);
    }
    fn state(&mut self, s: StageName) -> &mut StageState {
        match s {
            StageName::Compile => &mut self.view.compile,
            StageName::Transpile => &mut self.view.transpile,
            StageName::Build => &mut self.view.build,
        }
    }

    fn set(&mut self, s: StageName, status: StageStatus, error: Option<StageError>) {
        let now = unix_now();
        let st = self.state(s);
        st.status = status;
        if status == StageStatus::Running {
            st.started = Some(now);
        } else {
            st.ended = Some(now);
        }
        st.error = error;
        let snapshot = st.clone();
        self.save();
        self.emit(JobEvent::Stage {
            stage: s,
            state: snapshot,
        });
    }

    fn console(&mut self, stage: StageName, out: &RunOutput) {
        for (stream, text) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
            if text.trim().is_empty() {
                continue;
            }
            let line = ConsoleLine {
                stage,
                stream: stream.into(),
                text: tail(text),
            };
            self.view.console.push(line.clone());
            self.emit(JobEvent::Console(line));
        }
        self.save();
    }

    /// Runs one tool step with `work` (a `sandbox/` directory) as its only mount.
    async fn sandbox(
        &mut self,
        stage: StageName,
        tool: Tool,
        work: PathBuf,
    ) -> Result<RunOutput, StageError> {
        let _permit = self
            .store
            .builds
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| stage_err("internal", "the build queue is closed"))?;
        // The container runs as uid 10001 and bind mounts keep host uids: open the tree to it.
        let w = work.clone();
        tokio::task::spawn_blocking(move || make_world_writable(&w))
            .await
            .map_err(join_err)?
            .map_err(io_err)?;
        let out = self
            .store
            .runner
            .run(RunSpec {
                tool,
                work,
                timeout: timeout_for(&tool),
            })
            .await
            .map_err(|e| stage_err("internal", e.to_string()))?;
        self.console(stage, &out);
        if out.timed_out {
            return Err(stage_err(
                "timeout",
                format!(
                    "{} took longer than {} s",
                    tool_name(&tool),
                    timeout_for(&tool).as_secs()
                ),
            ));
        }
        Ok(out)
    }

    async fn run(mut self) {
        let result = async {
            self.set(StageName::Compile, StageStatus::Running, None);
            let compiled = self.compile().await.map_err(|e| (StageName::Compile, e))?;
            self.set(StageName::Compile, StageStatus::Done, None);
            self.set(StageName::Transpile, StageStatus::Running, None);
            let transpiled = self
                .transpile(compiled)
                .await
                .map_err(|e| (StageName::Transpile, e))?;
            self.set(StageName::Transpile, StageStatus::Done, None);
            self.set(StageName::Build, StageStatus::Running, None);
            self.build(transpiled)
                .await
                .map_err(|e| (StageName::Build, e))?;
            self.set(StageName::Build, StageStatus::Done, None);
            Ok::<(), (StageName, StageError)>(())
        }
        .await;
        if let Err((stage, e)) = result {
            self.set(stage, StageStatus::Failed, Some(e));
        }
        self.view.finished = true;
        self.save();
        self.store.running.lock().unwrap().remove(&self.view.id);
        self.completed = true;
        self.emit(JobEvent::Done);
    }

    async fn compile(&mut self) -> Result<Compiled, StageError> {
        let cdir = self.dir.join("compile");
        let sdir = cdir.join("sandbox");
        std::fs::create_dir_all(&cdir).map_err(io_err)?;
        let src = self.dir.join("source");
        match self.view.lang {
            Lang::Solidity => {
                let mut files = Vec::new();
                for e in std::fs::read_dir(&src).map_err(io_err)? {
                    let e = e.map_err(io_err)?;
                    files.push((
                        e.file_name().to_string_lossy().into_owned(),
                        std::fs::read_to_string(e.path()).map_err(io_err)?,
                    ));
                }
                files.sort();
                std::fs::create_dir_all(&sdir).map_err(io_err)?;
                std::fs::write(
                    sdir.join("input.json"),
                    stages::solc_input(&files).to_string(),
                )
                .map_err(io_err)?;
                let out = self
                    .sandbox(StageName::Compile, Tool::Solc, sdir.clone())
                    .await?;
                let output: serde_json::Value =
                    take_output(&sdir, "output.json", MAX_SOLC_OUTPUT, "compile")?
                        .and_then(|b| serde_json::from_slice(&b).ok())
                        .ok_or_else(|| {
                            stage_err(
                                "compile",
                                format!(
                                    "solc exited {} without output: {}",
                                    out.status,
                                    out.stderr.trim()
                                ),
                            )
                        })?;
                let r = stages::parse_solc_output(
                    &output,
                    &files,
                    self.view.options.contract.as_deref(),
                )?;
                std::fs::write(cdir.join("runtime.hex"), hex::encode(&r.runtime))
                    .map_err(io_err)?;
                std::fs::write(cdir.join("creation.hex"), &r.creation_hex).map_err(io_err)?;
                write_json(&cdir.join("abi.json"), &r.abi).map_err(io_err)?;
                let code = r.runtime.clone();
                let view = tokio::task::spawn_blocking(move || {
                    evm_analysis::compile_view(
                        &r.runtime,
                        &r.contract,
                        r.contracts,
                        r.creation_hex,
                        r.abi,
                    )
                })
                .await
                .map_err(join_err)?;
                write_json(&cdir.join("view.json"), &CompileView::Evm(view)).map_err(io_err)?;
                Ok(Compiled::Evm(code))
            }
            Lang::SolanaRust | Lang::SolanaElf => {
                let elf = if self.view.lang == Lang::SolanaRust {
                    std::fs::create_dir_all(sdir.join("src")).map_err(io_err)?;
                    for n in ["lib.rs", "Cargo.toml"] {
                        if src.join(n).exists() {
                            std::fs::copy(src.join(n), sdir.join("src").join(n)).map_err(io_err)?;
                        }
                    }
                    let out = self
                        .sandbox(StageName::Compile, Tool::BuildSbf, sdir.clone())
                        .await?;
                    let so = if out.status == 0 {
                        take_output(&sdir, "program.so", MAX_ELF_BYTES as u64, "compile")?
                    } else {
                        None
                    };
                    let Some(so) = so else {
                        let diagnostics = stages::rustc_diagnostics(&out.stdout);
                        let message = diagnostics
                            .iter()
                            .find(|d| d.severity == "error")
                            .map(|d| format!("{}:{}:{}: {}", d.file, d.line, d.col, d.message))
                            .unwrap_or_else(|| format!("cargo build-sbf exited {}", out.status));
                        return Err(StageError {
                            kind: "compile".into(),
                            message,
                            diagnostics,
                        });
                    };
                    std::fs::write(cdir.join("program.so"), &so).map_err(io_err)?;
                    so
                } else {
                    std::fs::copy(src.join("program.so"), cdir.join("program.so"))
                        .map_err(io_err)?;
                    std::fs::read(cdir.join("program.so")).map_err(io_err)?
                };
                let a = tokio::task::spawn_blocking(move || sbpf_analysis::analyse(&elf))
                    .await
                    .map_err(join_err)?
                    .map_err(|e| match e {
                        sbpf_analysis::SbpfError::Unsupported(w) => {
                            stage_err("unsupported", w.detail)
                        }
                        other => stage_err("compile", other.to_string()),
                    })?;
                write_json(
                    &cdir.join("view.json"),
                    &CompileView::Sbpf(a.compile.clone()),
                )
                .map_err(io_err)?;
                Ok(Compiled::Sbpf(Box::new(a)))
            }
        }
    }

    async fn transpile(&mut self, c: Compiled) -> Result<Transpiled, StageError> {
        let tdir = self.dir.join("transpile");
        std::fs::create_dir_all(&tdir).map_err(io_err)?;
        let (view, c_text, c_file) = match c {
            Compiled::Evm(code) => {
                let (chain, stage) = (self.view.options.chain_id, self.view.options.stage);
                let (v, c) = tokio::task::spawn_blocking(move || {
                    evm_analysis::transpile(&code, chain, stage)
                })
                .await
                .map_err(join_err)?
                .map_err(|e| match e {
                    evm_analysis::EvmTranspileError::Unsupported(w) => StageError {
                        kind: "unsupported".into(),
                        message: format!("{}: {}", w.detail, w.explanation),
                        diagnostics: vec![],
                    },
                    other => stage_err("transpile", other.to_string()),
                })?;
                (v, c, "contract.c")
            }
            Compiled::Sbpf(a) => {
                let a = *a;
                (a.transpile, a.program_c, "program.c")
            }
        };
        std::fs::write(tdir.join(c_file), &c_text).map_err(io_err)?;
        write_json(&tdir.join("op_map.json"), &view.op_map).map_err(io_err)?;
        write_json(&tdir.join("view.json"), &view).map_err(io_err)?;
        Ok(Transpiled {
            c_file,
            op_map: view.op_map,
        })
    }

    async fn build(&mut self, t: Transpiled) -> Result<(), StageError> {
        let bdir = self.dir.join("build");
        let sdir = bdir.join("sandbox");
        let shim = sdir.join("shim");
        let c_text =
            std::fs::read_to_string(self.dir.join("transpile").join(t.c_file)).map_err(io_err)?;
        if t.c_file == "contract.c" {
            stages::write_evm_shim(&shim, "contract", &c_text)
        } else {
            stages::write_sbpf_shim(&shim, "program", &c_text)
        }
        .map_err(io_err)?;
        let out = self
            .sandbox(
                StageName::Build,
                Tool::RandGuestBuild { debug: false },
                sdir.clone(),
            )
            .await?;
        // rand-guest still writes image.bin when its checker rejects the image (over the cap, a
        // forbidden instruction) and then exits 1: that image is shown, with fits and findings
        // from image::view. Only a build that wrote no image fails the stage.
        let Some(image) = take_output(&sdir, "image.bin", MAX_IMAGE, "build")? else {
            return Err(stage_err(
                "build",
                format!(
                    "rand-guest build exited {}: {}",
                    out.status,
                    out.stderr.lines().last().unwrap_or("")
                ),
            ));
        };
        std::fs::write(bdir.join("image.bin"), &image).map_err(io_err)?;
        // A rejected image's -g twin would be rejected too, before its line table is dumped.
        let dbg = if out.status == 0 {
            self.sandbox(
                StageName::Build,
                Tool::RandGuestBuild { debug: true },
                sdir.clone(),
            )
            .await
        } else {
            Err(stage_err("build", "the release image was rejected"))
        };
        let debug_line = match dbg {
            Ok(o) if o.status == 0 => {
                let dbg_image = take_output(&sdir, "image-dbg.bin", MAX_IMAGE, "build")?;
                let dl = take_output(&sdir, "debug-line.txt", MAX_DEBUG_LINE, "build")?;
                if let Some(d) = &dbg_image {
                    std::fs::write(bdir.join("image-dbg.bin"), d).map_err(io_err)?;
                }
                if let Some(d) = &dl {
                    std::fs::write(bdir.join("debug-line.txt"), d).map_err(io_err)?;
                }
                let same = dbg_image
                    .and_then(|d| {
                        Some((
                            rand_guest::pack::split(&image).ok()?.1,
                            rand_guest::pack::split(&d).ok()?.1,
                        ))
                    })
                    .map(|(a, b)| a == b)
                    .unwrap_or(false);
                if same {
                    dl.and_then(|b| String::from_utf8(b).ok())
                } else {
                    None
                }
            }
            _ => None,
        };
        let (cap, op_map, c_file) = (self.store.config.max_program_words, t.op_map, t.c_file);
        let had_debug = debug_line.is_some();
        let mut view = tokio::task::spawn_blocking(move || {
            image::view(&image, cap, &op_map, debug_line.as_deref(), c_file)
        })
        .await
        .map_err(join_err)?
        .map_err(|e| stage_err("build", e))?;
        if !had_debug {
            view.source_map_note = Some("the -g debug build did not run or did not reproduce the production code, so there is no pc-level source map; the opcode-to-C map still works".into());
        }
        write_json(&bdir.join("source_map.json"), &view.source_map).map_err(io_err)?;
        write_json(&bdir.join("view.json"), &view).map_err(io_err)?;
        Ok(())
    }
}

/// `chmod -R a+rwX`: everyone may read and write, directories (and files already executable by
/// someone) may be entered or executed. Symlinks are left alone, never followed.
///
/// The container runs as uid 10001, so a previous step can leave entries this process does not
/// own: those are skipped (only their owner may chmod them; the container's `umask 000` and its
/// closing `chmod -R a+rwX` already opened them), as is anything that vanishes or refuses while
/// walking. Only a missing or unreadable `p` itself is an error.
#[cfg(unix)]
pub fn make_world_writable(p: &Path) -> std::io::Result<()> {
    // SAFETY: geteuid has no preconditions and cannot fail.
    make_world_writable_as(p, unsafe { libc::geteuid() })
}

/// [`make_world_writable`] as if this process ran as `euid` (tests use it to stand in for the
/// container's uid, which an unprivileged test cannot create files as).
#[cfg(unix)]
pub fn make_world_writable_as(p: &Path, euid: u32) -> std::io::Result<()> {
    let m = std::fs::symlink_metadata(p)?;
    open_up(p, &m, euid);
    if m.is_dir() {
        for e in std::fs::read_dir(p)? {
            match e {
                Ok(e) => walk_open_up(&e.path(), euid),
                Err(e) => tracing::warn!("make_world_writable: {}: {e}", p.display()),
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn walk_open_up(p: &Path, euid: u32) {
    let Ok(m) = std::fs::symlink_metadata(p) else {
        return;
    };
    open_up(p, &m, euid);
    if !m.is_dir() {
        return;
    }
    match std::fs::read_dir(p) {
        Ok(rd) => rd.flatten().for_each(|e| walk_open_up(&e.path(), euid)),
        Err(e) => tracing::debug!("make_world_writable: skipping {}: {e}", p.display()),
    }
}

/// Adds a+rw (and a+x where anyone has x, or on directories) to an entry this uid owns.
#[cfg(unix)]
fn open_up(p: &Path, m: &std::fs::Metadata, euid: u32) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if m.file_type().is_symlink() || m.uid() != euid {
        return;
    }
    let mode = m.permissions().mode();
    let x = if m.is_dir() || mode & 0o111 != 0 {
        0o111
    } else {
        0
    };
    let want = mode | 0o666 | x;
    if want != mode {
        if let Err(e) = std::fs::set_permissions(p, std::fs::Permissions::from_mode(want)) {
            tracing::warn!("make_world_writable: {}: {e}", p.display());
        }
    }
}

#[cfg(not(unix))]
pub fn make_world_writable(_: &Path) -> std::io::Result<()> {
    Ok(())
}

fn io_err(e: std::io::Error) -> StageError {
    stage_err("internal", e.to_string())
}
fn join_err(e: tokio::task::JoinError) -> StageError {
    stage_err("internal", e.to_string())
}
fn tool_name(t: &Tool) -> &'static str {
    match t {
        Tool::Solc => "solc",
        Tool::BuildSbf => "cargo build-sbf",
        Tool::RandGuestBuild { .. } => "rand-guest build",
    }
}
