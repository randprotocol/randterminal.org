//! Runs: build the input vectors from the call form, emulate, fold the trace to disk.
use crate::error::{ApiResult, AppError};
use crate::jobs::{read_regular, write_json};
use crate::state::AppState;
use rand_zkvm::emulator::execute;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use terminal_core::views::*;
use terminal_core::{evm_call, image, sbpf_call, trace};

pub const MAX_CYCLES: usize = (1 << 20) - 1;
pub const WALL_CLOCK: Duration = Duration::from_secs(60);

#[derive(Serialize, Deserialize)]
struct RunMeta { job: String, trapped: Option<String> }

fn rid_ok(rid: &str) -> bool { rid.len() == 32 && rid.chars().all(|c| c.is_ascii_hexdigit()) }

pub async fn create(s: &AppState, job: &str, req: RunRequest) -> ApiResult<RunSummary> {
    let view = s.store.view(job)?;
    if view.build.status != StageStatus::Done { return Err(AppError::Conflict("the image is not built yet".into())); }
    let jdir = s.store.dir(job)?;
    let program = image::load(&read_regular(&jdir.join("build/image.bin"))?).map_err(AppError::Internal)?;
    let (inputs, public, discovery) = match (view.lang, req.call) {
        (Lang::Solidity, CallForm::Evm(form)) => {
            let code = hex::decode(String::from_utf8_lossy(&read_regular(&jdir.join("compile/runtime.hex"))?).trim()).map_err(|e| AppError::Internal(e.to_string()))?;
            let (c, steps) = tokio::task::spawn_blocking(move || evm_call::build(&code, &form)).await.map_err(|e| AppError::Internal(e.to_string()))?.map_err(AppError::BadRequest)?;
            (c.input_words(), vec![], steps)
        }
        (Lang::SolanaRust | Lang::SolanaElf, CallForm::Sbpf(form)) => {
            let elf = read_regular(&jdir.join("compile/program.so"))?;
            let c = sbpf_call::build(&elf, &form).map_err(AppError::BadRequest)?;
            (c.input_words(), c.public_words(), vec![])
        }
        (lang, _) => return Err(AppError::BadRequest(format!("this job is {lang:?}; send the matching call form"))),
    };
    let max = req.max_cycles.unwrap_or(MAX_CYCLES).min(MAX_CYCLES);
    let permit = s.runs.clone().try_acquire_owned().map_err(|_| AppError::Unavailable("every emulator slot is busy; try again shortly".into()))?;
    let rid = uuid::Uuid::new_v4().simple().to_string();
    let rdir = jdir.join("runs").join(&rid);
    std::fs::create_dir_all(&rdir)?;
    let (n_in, n_pub) = (inputs.len(), public.len());
    let rd = rdir.clone();
    let rid2 = rid.clone();
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        match execute(&program, &inputs, &public, max) {
            Ok(exec) => { trace::write(&rd, &exec)?; Ok::<_, std::io::Error>((trace::summarize(&rid2, &program, n_in, n_pub, &exec, discovery), None)) }
            Err(e) => Ok((trace::trapped(&rid2, &e, n_in, n_pub, discovery), Some(format!("{e:?}")))),
        }
    });
    let done = tokio::time::timeout(WALL_CLOCK, work).await;
    let summary_trapped = match done {
        Ok(Ok(Ok(v))) => v,
        Ok(Ok(Err(e))) => { let _ = std::fs::remove_dir_all(&rdir); return Err(e.into()); }
        Ok(Err(e)) => { let _ = std::fs::remove_dir_all(&rdir); return Err(AppError::Internal(e.to_string())); }
        Err(_) => { let _ = std::fs::remove_dir_all(&rdir); return Err(AppError::Unavailable("the run took longer than 60 s".into())); }
    };
    let (summary, trapped) = summary_trapped;
    write_json(&rdir.join("summary.json"), &summary)?;
    write_json(&rdir.join("run.json"), &RunMeta { job: job.to_string(), trapped })?;
    let index = s.config.data_dir.join("runs");
    std::fs::create_dir_all(&index)?;
    std::fs::write(index.join(&rid), job)?;
    Ok(summary)
}

pub fn open(s: &AppState, rid: &str) -> ApiResult<(PathBuf, PathBuf)> {
    if !rid_ok(rid) { return Err(AppError::NotFound(format!("run {rid}"))); }
    let index = s.config.data_dir.join("runs").join(rid);
    let job = std::fs::read_to_string(&index).map_err(|_| AppError::NotFound(format!("run {rid}")))?;
    let jdir = s.store.dir(job.trim())?;
    let rdir = jdir.join("runs").join(rid);
    if !rdir.join("summary.json").exists() { let _ = std::fs::remove_file(index); return Err(AppError::NotFound(format!("run {rid}"))); }
    s.store.touch(job.trim());
    Ok((rdir, jdir))
}

pub fn trace_of(rdir: &std::path::Path) -> ApiResult<trace::Trace> {
    let meta: RunMeta = serde_json::from_slice(&std::fs::read(rdir.join("run.json"))?)?;
    if let Some(t) = meta.trapped { return Err(AppError::Conflict(format!("the run trapped at {t}; there is no trace"))); }
    Ok(trace::Trace::open(rdir)?)
}

/// 400 for `InvalidInput` from the trace reader (out-of-range `at`, oversized page or window).
pub fn trace_err(e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::InvalidInput { AppError::BadRequest(e.to_string()) } else { AppError::Internal(e.to_string()) }
}
