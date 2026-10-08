//! Runs: build the input vectors from the call form, emulate, fold the trace to disk.
use crate::error::{ApiResult, AppError};
use crate::jobs::{read_regular, write_json};
use crate::state::AppState;
use rand_zkvm::emulator::execute;
use rand_zkvm::emulator::ExecError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use terminal_core::views::*;
use terminal_core::{evm_call, image, sbpf_call, trace};

pub const MAX_CYCLES: usize = (1 << 20) - 1;
pub const WALL_CLOCK: Duration = Duration::from_secs(60);

#[derive(Serialize, Deserialize)]
struct RunMeta {
    job: String,
    trapped: Option<String>,
}

fn rid_ok(rid: &str) -> bool {
    rid.len() == 32 && rid.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// Highest EVM gas limit a run may ask the host interpreter for.
pub const MAX_GAS: u64 = 30_000_000;

enum Outcome {
    Bad(String),
    Ran(Box<RunSummary>, Option<String>),
}

pub async fn create(s: &AppState, job: &str, req: RunRequest) -> ApiResult<RunSummary> {
    let view = s.store.view(job)?;
    s.store.ensure_space(Some(job)).await?;
    s.store.check_space()?;
    if view.build.status != StageStatus::Done {
        return Err(AppError::Conflict("the image is not built yet".into()));
    }
    let jdir = s.store.dir(job)?;
    let program =
        image::load(&read_regular(&jdir.join("build/image.bin"))?).map_err(AppError::Internal)?;
    enum Call {
        Evm(Vec<u8>, EvmCallForm),
        Sbpf(Vec<u8>, SbpfCallForm),
    }
    let call = match (view.lang, req.call) {
        (Lang::Solidity, CallForm::Evm(form)) => {
            if form.gas_limit > MAX_GAS {
                return Err(AppError::BadRequest(format!(
                    "gas_limit {} is above the limit of {MAX_GAS}",
                    form.gas_limit
                )));
            }
            let code = hex::decode(
                String::from_utf8_lossy(&read_regular(&jdir.join("compile/runtime.hex"))?).trim(),
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;
            Call::Evm(code, form)
        }
        (Lang::SolanaRust | Lang::SolanaElf, CallForm::Sbpf(form)) => {
            Call::Sbpf(read_regular(&jdir.join("compile/program.so"))?, form)
        }
        (lang, _) => {
            return Err(AppError::BadRequest(format!(
                "this job is {lang:?}; send the matching call form"
            )))
        }
    };
    let max = req.max_cycles.unwrap_or(MAX_CYCLES).min(MAX_CYCLES);
    let permit = s.runs.clone().try_acquire_owned().map_err(|_| {
        AppError::Unavailable("every emulator slot is busy; try again shortly".into())
    })?;
    let rid = uuid::Uuid::new_v4().simple().to_string();
    let rdir = jdir.join("runs").join(&rid);
    std::fs::create_dir_all(&rdir)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let (rd, rid2, flag) = (rdir.clone(), rid.clone(), cancelled.clone());
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let (inputs, public, discovery) = match call {
            Call::Evm(code, form) => match evm_call::build(&code, &form) {
                Ok((c, steps)) => (c.input_words(), vec![], steps),
                Err(m) => return Ok(Outcome::Bad(m)),
            },
            Call::Sbpf(elf, form) => match sbpf_call::build(&elf, &form) {
                Ok(c) => (c.input_words(), c.public_words(), vec![]),
                Err(m) => return Ok(Outcome::Bad(m)),
            },
        };
        let (n_in, n_pub) = (inputs.len(), public.len());
        match execute(&program, &inputs, &public, max) {
            Ok(exec) => {
                if flag.load(Ordering::SeqCst) {
                    return Ok(Outcome::Bad("cancelled".into()));
                }
                trace::write(&rd, &exec)?;
                if flag.load(Ordering::SeqCst) {
                    let _ = std::fs::remove_dir_all(&rd);
                }
                Ok::<_, std::io::Error>(Outcome::Ran(
                    Box::new(trace::summarize(
                        &rid2, &program, n_in, n_pub, &exec, discovery,
                    )),
                    None,
                ))
            }
            Err(e) => {
                let msg = match e {
                    ExecError::OutOfCycles(n) => format!("the run ran out of cycles at {n}"),
                    other => format!("the run trapped: {other:?}"),
                };
                Ok(Outcome::Ran(
                    Box::new(trace::trapped(&rid2, &e, n_in, n_pub, discovery)),
                    Some(msg),
                ))
            }
        }
    });
    let done = tokio::time::timeout(WALL_CLOCK, work).await;
    let fail = |e: AppError| {
        cancelled.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_dir_all(&rdir);
        e
    };
    let (summary, trapped) = match done {
        Ok(Ok(Ok(Outcome::Ran(sum, t)))) => (*sum, t),
        Ok(Ok(Ok(Outcome::Bad(m)))) => return Err(fail(AppError::BadRequest(m))),
        Ok(Ok(Err(e))) => return Err(fail(e.into())),
        Ok(Err(e)) => return Err(fail(AppError::Internal(e.to_string()))),
        Err(_) => {
            return Err(fail(AppError::Unavailable(
                "the run took longer than 60 s".into(),
            )))
        }
    };
    write_json(&rdir.join("summary.json"), &summary)?;
    write_json(
        &rdir.join("run.json"),
        &RunMeta {
            job: job.to_string(),
            trapped,
        },
    )?;
    let index = s.config.data_dir.join("runs");
    std::fs::create_dir_all(&index)?;
    std::fs::write(index.join(&rid), job)?;
    let (rd, runs_dir) = (rdir.clone(), jdir.join("runs"));
    let bytes = tokio::task::spawn_blocking(move || {
        prune_runs(&runs_dir, &index);
        crate::jobs::dir_size(&rd)
    })
    .await
    .unwrap_or(0);
    s.store.add_usage(bytes);
    Ok(summary)
}

/// Runs a job keeps; older ones are deleted when a new one finishes.
pub const MAX_RUNS_PER_JOB: usize = 8;

/// Deletes all but the newest [`MAX_RUNS_PER_JOB`] finished runs (those with `run.json`; a run
/// still emulating has none and is left alone) and their index files.
fn prune_runs(runs_dir: &std::path::Path, index: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(runs_dir) else {
        return;
    };
    let mut done: Vec<(std::time::SystemTime, String)> = rd
        .flatten()
        .filter_map(|e| {
            let rid = e.file_name().into_string().ok().filter(|r| rid_ok(r))?;
            let at = std::fs::symlink_metadata(e.path().join("run.json"))
                .ok()?
                .modified()
                .ok()?;
            Some((at, rid))
        })
        .collect();
    if done.len() <= MAX_RUNS_PER_JOB {
        return;
    }
    done.sort();
    let old = done.len() - MAX_RUNS_PER_JOB;
    for (_, rid) in done.into_iter().take(old) {
        if let Err(e) = std::fs::remove_dir_all(runs_dir.join(&rid)) {
            tracing::warn!("could not delete run {rid}: {e}");
        }
        let _ = std::fs::remove_file(index.join(&rid));
    }
}

pub fn open(s: &AppState, rid: &str) -> ApiResult<(PathBuf, PathBuf)> {
    if !rid_ok(rid) {
        return Err(AppError::NotFound(format!("run {rid}")));
    }
    let index = s.config.data_dir.join("runs").join(rid);
    let job =
        std::fs::read_to_string(&index).map_err(|_| AppError::NotFound(format!("run {rid}")))?;
    let jdir = s.store.dir(job.trim())?;
    let rdir = jdir.join("runs").join(rid);
    if !rdir.join("summary.json").exists() {
        let _ = std::fs::remove_file(index);
        return Err(AppError::NotFound(format!("run {rid}")));
    }
    s.store.touch(job.trim());
    Ok((rdir, jdir))
}

pub fn trace_of(rdir: &std::path::Path) -> ApiResult<trace::Trace> {
    let meta: RunMeta = serde_json::from_slice(&std::fs::read(rdir.join("run.json"))?)?;
    if let Some(t) = meta.trapped {
        return Err(AppError::Conflict(format!("{t}; there is no trace")));
    }
    Ok(trace::Trace::open(rdir)?)
}

/// 400 for `InvalidInput` from the trace reader (out-of-range `at`, oversized page or window).
pub fn trace_err(e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::InvalidInput {
        AppError::BadRequest(e.to_string())
    } else {
        AppError::Internal(e.to_string())
    }
}
