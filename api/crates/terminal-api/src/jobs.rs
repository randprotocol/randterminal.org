//! Jobs on disk and the pipeline that fills them. One directory per job (layout at the top of the
//! plan); `job.json` is rewritten after every change and every change is broadcast for SSE.
use crate::error::{ApiResult, AppError};
use crate::ratelimit::unix_now;
use crate::ApiConfig;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use terminal_core::views::*;
use terminal_core::{evm_analysis, image, sbpf_analysis, stages};
use terminal_sandbox::{timeout_for, RunOutput, RunSpec, Runner, Tool};
use tokio::sync::{broadcast, Semaphore};

pub const MAX_TEXT_BYTES: usize = 512 * 1024;
pub const MAX_ELF_BYTES: usize = 2 * 1024 * 1024;
const CONSOLE_KEEP: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent { Stage { stage: StageName, state: StageState }, Console(ConsoleLine), Done }

pub struct JobStore {
    root: PathBuf,
    config: Arc<ApiConfig>,
    runner: Arc<dyn Runner>,
    builds: Arc<Semaphore>,
    chans: Mutex<HashMap<String, broadcast::Sender<JobEvent>>>,
    access: Mutex<HashMap<String, u64>>,
}

fn queued() -> StageState { StageState { status: StageStatus::Queued, started: None, ended: None, error: None } }
fn stage_err(kind: &str, message: impl Into<String>) -> StageError { StageError { kind: kind.into(), message: message.into(), diagnostics: vec![] } }
fn valid_name(n: &str) -> bool { !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)) && !n.starts_with('.') }
fn tail(s: &str) -> String { if s.len() <= CONSOLE_KEEP { s.to_string() } else { let mut i = s.len() - CONSOLE_KEEP; while !s.is_char_boundary(i) { i += 1; } s[i..].to_string() } }

impl JobStore {
    pub fn new(config: Arc<ApiConfig>, runner: Arc<dyn Runner>) -> Arc<Self> {
        // The docker runner only mounts absolute paths, so resolve the data dir once here.
        let _ = std::fs::create_dir_all(config.data_dir.join("jobs"));
        let root = std::fs::canonicalize(&config.data_dir)
            .or_else(|_| std::env::current_dir().map(|c| c.join(&config.data_dir)))
            .unwrap_or_else(|_| config.data_dir.clone())
            .join("jobs");
        Arc::new(Self { root, builds: Arc::new(Semaphore::new(config.max_builds.max(1))), config, runner, chans: Mutex::new(HashMap::new()), access: Mutex::new(HashMap::new()) })
    }

    pub fn dir(&self, id: &str) -> PathBuf { self.root.join(id) }

    fn check_id(&self, id: &str) -> ApiResult<PathBuf> {
        if id.len() != 32 || !id.chars().all(|c| c.is_ascii_hexdigit()) { return Err(AppError::NotFound(format!("job {id}"))); }
        let d = self.dir(id);
        if !d.join("job.json").exists() { return Err(AppError::NotFound(format!("job {id}"))); }
        Ok(d)
    }

    pub fn touch(&self, id: &str) { self.access.lock().unwrap().insert(id.to_string(), unix_now()); }

    pub fn create(self: &Arc<Self>, req: NewJob) -> ApiResult<String> {
        if !(1..=2).contains(&req.options.stage) { return Err(AppError::BadRequest(format!("stage must be 1 or 2, not {}", req.options.stage))); }
        let mut text_total = 0usize;
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for f in &req.files {
            if !valid_name(&f.name) { return Err(AppError::BadRequest(format!("bad file name {:?}", f.name))); }
            let bytes = match f.encoding.as_str() {
                "text" => { text_total += f.content.len(); f.content.clone().into_bytes() }
                "base64" => base64::engine::general_purpose::STANDARD.decode(&f.content).map_err(|e| AppError::BadRequest(format!("{}: {e}", f.name)))?,
                e => return Err(AppError::BadRequest(format!("encoding {e:?}"))),
            };
            files.push((f.name.clone(), bytes));
        }
        if text_total > MAX_TEXT_BYTES { return Err(AppError::PayloadTooLarge(format!("{text_total} bytes of source; the limit is {MAX_TEXT_BYTES}"))); }
        let names: Vec<&str> = req.files.iter().map(|f| f.name.as_str()).collect();
        match req.lang {
            Lang::Solidity => {
                if files.is_empty() || files.len() > 16 { return Err(AppError::BadRequest("a Solidity job has 1 to 16 .sol files".into())); }
                if let Some(f) = req.files.iter().find(|f| !f.name.ends_with(".sol") || f.encoding != "text") { return Err(AppError::BadRequest(format!("{}: Solidity jobs take .sol text files", f.name))); }
            }
            Lang::SolanaRust => {
                if !names.contains(&"lib.rs") || names.iter().any(|n| *n != "lib.rs" && *n != "Cargo.toml") || req.files.iter().any(|f| f.encoding != "text") {
                    return Err(AppError::BadRequest("a Solana Rust job is lib.rs and optionally Cargo.toml, as text".into()));
                }
            }
            Lang::SolanaElf => {
                if names != ["program.so"] || req.files[0].encoding != "base64" { return Err(AppError::BadRequest("a Solana ELF job is one base64 file named program.so".into())); }
                if files[0].1.len() > MAX_ELF_BYTES { return Err(AppError::PayloadTooLarge(format!("{} bytes of ELF; the limit is {MAX_ELF_BYTES}", files[0].1.len()))); }
            }
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = self.dir(&id);
        std::fs::create_dir_all(dir.join("source"))?;
        for (n, b) in &files { std::fs::write(dir.join("source").join(n), b)?; }
        let view = JobView { id: id.clone(), lang: req.lang, options: req.options, created: unix_now(), compile: queued(), transpile: queued(), build: queued(), console: vec![], finished: false };
        write_json(&dir.join("job.json"), &view)?;
        let (tx, _) = broadcast::channel(256);
        self.chans.lock().unwrap().insert(id.clone(), tx);
        self.touch(&id);
        let me = self.clone();
        tokio::spawn(async move { Pipeline { store: me, dir, view }.run().await });
        Ok(id)
    }

    pub fn view(&self, id: &str) -> ApiResult<JobView> {
        let d = self.check_id(id)?;
        self.touch(id);
        Ok(serde_json::from_slice(&std::fs::read(d.join("job.json"))?)?)
    }

    pub fn stage_view(&self, id: &str, stage: &str) -> ApiResult<serde_json::Value> {
        let d = self.check_id(id)?;
        let sub = match stage { "compile" => "compile", "transpile" => "transpile", "image" => "build", _ => return Err(AppError::NotFound(stage.into())) };
        let p = d.join(sub).join("view.json");
        if !p.exists() { return Err(AppError::Conflict(format!("the {stage} view is not ready"))); }
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
        if !p.exists() { return Err(AppError::NotFound(name.into())); }
        Ok((std::fs::read(p)?, ty))
    }

    pub fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<JobEvent>> { self.chans.lock().unwrap().get(id).map(|t| t.subscribe()) }

    fn last_access(&self, id: &str) -> u64 {
        if let Some(t) = self.access.lock().unwrap().get(id) { return *t; }
        std::fs::metadata(self.dir(id).join("job.json")).and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
    }

    fn remove(&self, id: &str) {
        let _ = std::fs::remove_dir_all(self.dir(id));
        self.access.lock().unwrap().remove(id);
        self.chans.lock().unwrap().remove(id);
    }

    pub fn sweep(&self, now: u64) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.root) else { return 0 };
        let mut jobs: Vec<(String, u64, u64)> = rd.filter_map(|e| e.ok()).map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            let size = dir_size(&e.path());
            let at = self.last_access(&id);
            (id, at, size)
        }).collect();
        let mut removed = 0;
        jobs.retain(|(id, at, _)| if now.saturating_sub(*at) > self.config.job_ttl_secs { self.remove(id); removed += 1; false } else { true });
        let mut total: u64 = jobs.iter().map(|j| j.2).sum();
        jobs.sort_by_key(|j| j.1);
        for (id, _, size) in jobs {
            if total <= self.config.data_cap_bytes { break; }
            self.remove(&id); removed += 1; total -= size;
        }
        removed
    }

    pub fn spawn_sweeper(self: Arc<Self>) {
        tokio::spawn(async move { loop { tokio::time::sleep(std::time::Duration::from_secs(300)).await; let n = self.sweep(unix_now()); if n > 0 { tracing::info!("swept {n} job(s)"); } } });
    }
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map(|rd| rd.filter_map(|e| e.ok()).map(|e| { let m = e.metadata().ok(); if m.as_ref().map(|m| m.is_dir()).unwrap_or(false) { dir_size(&e.path()) } else { m.map(|m| m.len()).unwrap_or(0) } }).sum()).unwrap_or(0)
}

pub fn write_json<T: Serialize>(p: &Path, v: &T) -> std::io::Result<()> {
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(v)?)?;
    std::fs::rename(tmp, p)
}

struct Pipeline { store: Arc<JobStore>, dir: PathBuf, view: JobView }

/// What the compile stage hands the transpile stage.
enum Compiled { Evm(Vec<u8>), Sbpf(Box<sbpf_analysis::Analysis>) }
/// What the transpile stage hands the build stage.
struct Transpiled { c_file: &'static str, op_map: Vec<OpLine> }

impl Pipeline {
    fn emit(&self, e: JobEvent) { if let Some(tx) = self.store.chans.lock().unwrap().get(&self.view.id) { let _ = tx.send(e); } }
    fn save(&self) { let _ = write_json(&self.dir.join("job.json"), &self.view); }
    fn state(&mut self, s: StageName) -> &mut StageState { match s { StageName::Compile => &mut self.view.compile, StageName::Transpile => &mut self.view.transpile, StageName::Build => &mut self.view.build } }

    fn set(&mut self, s: StageName, status: StageStatus, error: Option<StageError>) {
        let now = unix_now();
        let st = self.state(s);
        st.status = status;
        if status == StageStatus::Running { st.started = Some(now); } else { st.ended = Some(now); }
        st.error = error;
        let snapshot = st.clone();
        self.save();
        self.emit(JobEvent::Stage { stage: s, state: snapshot });
    }

    fn console(&mut self, stage: StageName, out: &RunOutput) {
        for (stream, text) in [("stdout", &out.stdout), ("stderr", &out.stderr)] {
            if text.trim().is_empty() { continue; }
            let line = ConsoleLine { stage, stream: stream.into(), text: tail(text) };
            self.view.console.push(line.clone());
            self.emit(JobEvent::Console(line));
        }
        self.save();
    }

    async fn sandbox(&mut self, stage: StageName, tool: Tool, work: PathBuf) -> Result<RunOutput, StageError> {
        let _permit = self.store.builds.clone().acquire_owned().await.map_err(|_| stage_err("build", "the build queue is closed"))?;
        // The container runs as uid 10001 and bind mounts keep host uids: open the tree to it.
        make_world_writable(&work).map_err(io_err)?;
        let out = self.store.runner.run(RunSpec { tool, work, timeout: timeout_for(&tool) }).await.map_err(|e| stage_err(stage_kind(stage), e.to_string()))?;
        self.console(stage, &out);
        if out.timed_out { return Err(stage_err("timeout", format!("{} took longer than {} s", tool_name(&tool), timeout_for(&tool).as_secs()))); }
        Ok(out)
    }

    async fn run(mut self) {
        let result = async {
            self.set(StageName::Compile, StageStatus::Running, None);
            let compiled = self.compile().await.map_err(|e| (StageName::Compile, e))?;
            self.set(StageName::Compile, StageStatus::Done, None);
            self.set(StageName::Transpile, StageStatus::Running, None);
            let transpiled = self.transpile(compiled).await.map_err(|e| (StageName::Transpile, e))?;
            self.set(StageName::Transpile, StageStatus::Done, None);
            self.set(StageName::Build, StageStatus::Running, None);
            self.build(transpiled).await.map_err(|e| (StageName::Build, e))?;
            self.set(StageName::Build, StageStatus::Done, None);
            Ok::<(), (StageName, StageError)>(())
        }.await;
        if let Err((stage, e)) = result { self.set(stage, StageStatus::Failed, Some(e)); }
        self.view.finished = true;
        self.save();
        self.emit(JobEvent::Done);
    }

    async fn compile(&mut self) -> Result<Compiled, StageError> {
        let cdir = self.dir.join("compile");
        std::fs::create_dir_all(&cdir).map_err(io_err)?;
        let src = self.dir.join("source");
        match self.view.lang {
            Lang::Solidity => {
                let mut files = Vec::new();
                for e in std::fs::read_dir(&src).map_err(io_err)? { let e = e.map_err(io_err)?; files.push((e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).map_err(io_err)?)); }
                files.sort();
                std::fs::write(cdir.join("input.json"), stages::solc_input(&files).to_string()).map_err(io_err)?;
                let out = self.sandbox(StageName::Compile, Tool::Solc, cdir.clone()).await?;
                let output: serde_json::Value = std::fs::read(cdir.join("output.json")).ok().and_then(|b| serde_json::from_slice(&b).ok())
                    .ok_or_else(|| stage_err("compile", format!("solc exited {} without output: {}", out.status, out.stderr.trim())))?;
                let r = stages::parse_solc_output(&output, &files, self.view.options.contract.as_deref())?;
                std::fs::write(cdir.join("runtime.hex"), hex::encode(&r.runtime)).map_err(io_err)?;
                std::fs::write(cdir.join("creation.hex"), &r.creation_hex).map_err(io_err)?;
                write_json(&cdir.join("abi.json"), &r.abi).map_err(io_err)?;
                let code = r.runtime.clone();
                let view = tokio::task::spawn_blocking(move || evm_analysis::compile_view(&r.runtime, &r.contract, r.contracts, r.creation_hex, r.abi)).await.map_err(join_err)?;
                write_json(&cdir.join("view.json"), &CompileView::Evm(view)).map_err(io_err)?;
                Ok(Compiled::Evm(code))
            }
            Lang::SolanaRust | Lang::SolanaElf => {
                if self.view.lang == Lang::SolanaRust {
                    std::fs::create_dir_all(cdir.join("src")).map_err(io_err)?;
                    for n in ["lib.rs", "Cargo.toml"] { if src.join(n).exists() { std::fs::copy(src.join(n), cdir.join("src").join(n)).map_err(io_err)?; } }
                    let out = self.sandbox(StageName::Compile, Tool::BuildSbf, cdir.clone()).await?;
                    if out.status != 0 || !cdir.join("program.so").exists() {
                        let diagnostics = stages::rustc_diagnostics(&out.stdout);
                        let message = diagnostics.iter().find(|d| d.severity == "error").map(|d| format!("{}:{}:{}: {}", d.file, d.line, d.col, d.message)).unwrap_or_else(|| format!("cargo build-sbf exited {}", out.status));
                        return Err(StageError { kind: "compile".into(), message, diagnostics });
                    }
                } else {
                    std::fs::copy(src.join("program.so"), cdir.join("program.so")).map_err(io_err)?;
                }
                let elf = std::fs::read(cdir.join("program.so")).map_err(io_err)?;
                let a = tokio::task::spawn_blocking(move || sbpf_analysis::analyse(&elf)).await.map_err(join_err)?.map_err(|e| match e {
                    sbpf_analysis::SbpfError::Unsupported(w) => stage_err("unsupported", w.detail),
                    other => stage_err("compile", other.to_string()),
                })?;
                write_json(&cdir.join("view.json"), &CompileView::Sbpf(a.compile.clone())).map_err(io_err)?;
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
                let (v, c) = tokio::task::spawn_blocking(move || evm_analysis::transpile(&code, chain, stage)).await.map_err(join_err)?.map_err(|e| match e {
                    evm_analysis::EvmTranspileError::Unsupported(w) => StageError { kind: "unsupported".into(), message: format!("{}: {}", w.detail, w.explanation), diagnostics: vec![] },
                    other => stage_err("transpile", other.to_string()),
                })?;
                (v, c, "contract.c")
            }
            Compiled::Sbpf(a) => { let a = *a; (a.transpile, a.program_c, "program.c") }
        };
        std::fs::write(tdir.join(c_file), &c_text).map_err(io_err)?;
        write_json(&tdir.join("op_map.json"), &view.op_map).map_err(io_err)?;
        write_json(&tdir.join("view.json"), &view).map_err(io_err)?;
        Ok(Transpiled { c_file, op_map: view.op_map })
    }

    async fn build(&mut self, t: Transpiled) -> Result<(), StageError> {
        let bdir = self.dir.join("build");
        let shim = bdir.join("shim");
        let c_text = std::fs::read_to_string(self.dir.join("transpile").join(t.c_file)).map_err(io_err)?;
        if t.c_file == "contract.c" { stages::write_evm_shim(&shim, "contract", &c_text) } else { stages::write_sbpf_shim(&shim, "program", &c_text) }.map_err(io_err)?;
        let out = self.sandbox(StageName::Build, Tool::RandGuestBuild { debug: false }, bdir.clone()).await?;
        if out.status != 0 || !bdir.join("image.bin").exists() {
            return Err(stage_err("build", format!("rand-guest build exited {}: {}", out.status, out.stderr.lines().last().unwrap_or(""))));
        }
        let dbg = self.sandbox(StageName::Build, Tool::RandGuestBuild { debug: true }, bdir.clone()).await;
        let image = std::fs::read(bdir.join("image.bin")).map_err(io_err)?;
        let debug_line = match dbg {
            Ok(o) if o.status == 0 => {
                let same = std::fs::read(bdir.join("image-dbg.bin")).ok().and_then(|d| Some((rand_guest::pack::split(&image).ok()?.1, rand_guest::pack::split(&d).ok()?.1))).map(|(a, b)| a == b).unwrap_or(false);
                if same { std::fs::read_to_string(bdir.join("debug-line.txt")).ok() } else { None }
            }
            _ => None,
        };
        let (cap, op_map, c_file) = (self.store.config.max_program_words, t.op_map, t.c_file);
        let had_debug = debug_line.is_some();
        let mut view = tokio::task::spawn_blocking(move || image::view(&image, cap, &op_map, debug_line.as_deref(), c_file)).await.map_err(join_err)?.map_err(|e| stage_err("build", e))?;
        if !had_debug { view.source_map_note = Some("the -g debug build did not run or did not reproduce the production code, so there is no pc-level source map; the opcode-to-C map still works".into()); }
        write_json(&bdir.join("source_map.json"), &view.source_map).map_err(io_err)?;
        write_json(&bdir.join("view.json"), &view).map_err(io_err)?;
        Ok(())
    }
}

/// `chmod -R a+rwX`: everyone may read and write, directories (and files already executable by
/// someone) may be entered or executed. Symlinks are left alone, never followed.
#[cfg(unix)]
pub fn make_world_writable(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let m = std::fs::symlink_metadata(p)?;
    if m.file_type().is_symlink() { return Ok(()); }
    let mode = m.permissions().mode();
    let x = if m.is_dir() || mode & 0o111 != 0 { 0o111 } else { 0 };
    let want = mode | 0o666 | x;
    if want != mode { std::fs::set_permissions(p, std::fs::Permissions::from_mode(want))?; }
    if m.is_dir() { for e in std::fs::read_dir(p)? { make_world_writable(&e?.path())?; } }
    Ok(())
}

#[cfg(not(unix))]
pub fn make_world_writable(_: &Path) -> std::io::Result<()> { Ok(()) }

fn io_err(e: std::io::Error) -> StageError { stage_err("internal", e.to_string()) }
fn join_err(e: tokio::task::JoinError) -> StageError { stage_err("internal", e.to_string()) }
fn stage_kind(s: StageName) -> &'static str { match s { StageName::Compile => "compile", StageName::Transpile => "transpile", StageName::Build => "build" } }
fn tool_name(t: &Tool) -> &'static str { match t { Tool::Solc => "solc", Tool::BuildSbf => "cargo build-sbf", Tool::RandGuestBuild { .. } => "rand-guest build" } }
