//! Parallel restore job manager.
//!
//! The GUI process starts one shared usbmuxd (a no-op on macOS) and then spawns
//! a separate worker *process* per device (a self-exec of this binary; see
//! [`crate::worker`]). Each worker gets its own copy of idevicerestore's global
//! C state, so restores run genuinely in parallel. A semaphore caps how many run
//! at once — several on macOS, one elsewhere (Linux/Windows would need a shared
//! usbmuxd hardened for concurrent clients first).

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{Mutex, Semaphore};
use tokio::task::AbortHandle;

/// How many restores may run at once. idevicerestore's global state confines
/// each to its own process; on Linux/Windows the shared usbmuxd isn't hardened
/// for concurrent clients yet, so those run serially.
const MAX_PARALLEL: usize = if cfg!(target_os = "macos") { 4 } else { 1 };

/// A serializable snapshot of one restore job, pushed to the UI.
#[derive(Clone, Serialize)]
pub struct JobView {
    pub id: u64,
    pub name: String,
    pub ecid: String,
    /// queued | running | done | failed | canceled
    pub status: String,
    pub step: String,
    pub progress: f32,
    pub message: String,
    /// Erase-restore key-wipe verdict once known (`confirmed` | `failed` |
    /// `unconfirmed` | `not_applicable`); `None` until the worker reports it.
    pub obliteration: Option<String>,
    /// Full checkpoint messages the device reported, each a JSON array of
    /// strings: `checkpoints_json` is the compact JSON view, `checkpoints_raw`
    /// the exact plists as XML. `None` until reported.
    pub checkpoints_json: Option<String>,
    pub checkpoints_raw: Option<String>,
}

struct Job {
    view: JobView,
    abort: Option<AbortHandle>,
    // Retained so a job can be restarted.
    ipsw: String,
    // "restore" | "revive" | "obliterate".
    mode: String,
}

#[derive(Default)]
struct Inner {
    jobs: HashMap<u64, Job>,
    order: Vec<u64>,
    next_id: u64,
    /// Held for the app's lifetime so child workers reuse one usbmuxd.
    shared_usbmuxd: Option<restorekit::SharedUsbmuxd>,
}

#[derive(Clone)]
pub struct Restores {
    inner: Arc<Mutex<Inner>>,
    sem: Arc<Semaphore>,
    /// A dedicated runtime with the full I/O + signal drivers, so `tokio::process`
    /// child reaping works regardless of how Tauri configures its own runtime.
    rt: Arc<tokio::runtime::Runtime>,
}

impl Restores {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build restore runtime");
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            sem: Arc::new(Semaphore::new(MAX_PARALLEL)),
            rt: Arc::new(rt),
        }
    }
}

fn emit_update(app: &AppHandle, view: &JobView) {
    let _ = app.emit("restore_job_update", view);
}

fn emit_log(app: &AppHandle, id: u64, level: i32, line: &str) {
    let _ = app.emit(
        "restore_job_log",
        serde_json::json!({ "id": id, "level": level, "line": line }),
    );
}

/// Update a job's view and push it to the UI. Returns false if the job is gone
/// or already canceled (so the worker loop can bail).
async fn set_status(
    inner: &Arc<Mutex<Inner>>,
    app: &AppHandle,
    id: u64,
    status: &str,
    step: &str,
    progress: Option<f32>,
    message: &str,
) -> bool {
    let mut g = inner.lock().await;
    let Some(job) = g.jobs.get_mut(&id) else {
        return false;
    };
    if job.view.status == "canceled" {
        return false;
    }
    job.view.status = status.to_string();
    job.view.step = step.to_string();
    if let Some(p) = progress {
        job.view.progress = p;
    }
    if !message.is_empty() {
        job.view.message = message.to_string();
    }
    let view = job.view.clone();
    drop(g);
    emit_update(app, &view);
    true
}

/// Run one restore worker process to completion, streaming its NDJSON events to
/// the UI as job updates and log lines.
async fn run_job(
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    sem: Arc<Semaphore>,
    id: u64,
    ipsw: String,
    ecid: String,
    mode: String,
) {
    let _permit = match sem.acquire_owned().await {
        Ok(p) => p,
        Err(_) => return,
    };
    if !set_status(&inner, &app, id, "running", "starting", Some(0.0), "").await {
        return;
    }

    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            set_status(&inner, &app, id, "failed", "failed", None, &e.to_string()).await;
            return;
        }
    };
    let mut child = match tokio::process::Command::new(exe)
        .arg(crate::worker::RESTORE_WORKER_ARG)
        .args(["--ipsw", &ipsw, "--ecid", &ecid, "--mode", &mode])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            set_status(&inner, &app, id, "failed", "failed", None, &e.to_string()).await;
            return;
        }
    };

    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();
    let mut failure: Option<String> = None;
    let mut obliteration: Option<String> = None;
    let mut checkpoints_json: Option<String> = None;
    let mut checkpoints_raw: Option<String> = None;

    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        match v.get("event").and_then(|e| e.as_str()) {
            Some("restore_step") => {
                let step = v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let progress = v.get("progress").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32 * 100.0;
                set_status(&inner, &app, id, "running", &step, Some(progress), "").await;
            }
            Some("restore_retrying") => {
                let msg = v.get("message").and_then(|x| x.as_str()).unwrap_or("");
                emit_log(&app, id, 1, &format!("transient failure, retrying: {msg}"));
            }
            Some("log_line") => {
                let level = v.get("level").and_then(|x| x.as_i64()).unwrap_or(3) as i32;
                let l = v.get("line").and_then(|x| x.as_str()).unwrap_or("");
                emit_log(&app, id, level, l);
            }
            Some("obliteration") => {
                let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("");
                if !status.is_empty() {
                    // Keep a confirmed/failed verdict over a later unconfirmed one
                    // (a retry that fails before re-reaching the wipe checkpoint).
                    let strong = status == "confirmed" || status == "failed";
                    if obliteration.is_none() || strong {
                        obliteration = Some(status.to_string());
                    }
                }
                let detail = v.get("detail").and_then(|x| x.as_str()).unwrap_or("");
                emit_log(
                    &app,
                    id,
                    if status == "failed" { 0 } else { 2 },
                    &format!(
                        "encryption-key obliteration: {status}{}",
                        if detail.is_empty() {
                            String::new()
                        } else {
                            format!(" ({detail})")
                        }
                    ),
                );
            }
            Some("checkpoints") => {
                // Store each array (of full checkpoint plists) verbatim as a JSON
                // string for the history record.
                if let Some(j) = v.get("json").filter(|a| a.as_array().is_some_and(|a| !a.is_empty())) {
                    checkpoints_json = Some(j.to_string());
                }
                if let Some(r) = v.get("raw").filter(|a| a.as_array().is_some_and(|a| !a.is_empty())) {
                    checkpoints_raw = Some(r.to_string());
                }
            }
            Some("error") => {
                failure = Some(
                    v.get("message")
                        .and_then(|x| x.as_str())
                        .unwrap_or("restore failed")
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    let ok = matches!(child.wait().await, Ok(s) if s.success());
    // Record the wipe verdict on the job before the terminal update so the UI's
    // "done" snapshot (and the history entry it writes) carries it.
    if obliteration.is_some() || checkpoints_json.is_some() || checkpoints_raw.is_some() {
        let mut g = inner.lock().await;
        if let Some(job) = g.jobs.get_mut(&id) {
            if obliteration.is_some() {
                job.view.obliteration = obliteration;
            }
            job.view.checkpoints_json = checkpoints_json;
            job.view.checkpoints_raw = checkpoints_raw;
        }
    }
    if ok && failure.is_none() {
        set_status(&inner, &app, id, "done", "restored", Some(100.0), "").await;
    } else {
        let msg = failure.unwrap_or_else(|| "restore process exited abnormally".into());
        set_status(&inner, &app, id, "failed", "failed", None, &msg).await;
    }
}

/// Enqueue a restore of `ipsw` onto the device with `ecid`. Returns the job id.
/// The firmware must already be present (resolve/download it first).
#[tauri::command]
pub async fn enqueue_restore(
    app: AppHandle,
    restores: State<'_, Restores>,
    ipsw: String,
    ecid: String,
    name: String,
    mode: String,
) -> Result<u64, String> {
    let inner = restores.inner.clone();
    let sem = restores.sem.clone();

    let id = {
        let mut g = inner.lock().await;
        if g.shared_usbmuxd.is_none() {
            match restorekit::start_shared_usbmuxd(&mut |_| {}) {
                Ok(s) => g.shared_usbmuxd = Some(s),
                Err(e) => return Err(e.to_string()),
            }
        }
        g.next_id += 1;
        let id = g.next_id;
        let view = JobView {
            id,
            name,
            ecid: ecid.clone(),
            status: "queued".into(),
            step: "queued".into(),
            progress: 0.0,
            message: String::new(),
            obliteration: None,
            checkpoints_json: None,
            checkpoints_raw: None,
        };
        g.jobs.insert(
            id,
            Job {
                view: view.clone(),
                abort: None,
                ipsw: ipsw.clone(),
                mode: mode.clone(),
            },
        );
        g.order.push(id);
        emit_update(&app, &view);
        id
    };

    let handle = restores
        .rt
        .spawn(run_job(app, inner.clone(), sem, id, ipsw, ecid, mode));
    if let Some(job) = inner.lock().await.jobs.get_mut(&id) {
        job.abort = Some(handle.abort_handle());
    }
    Ok(id)
}

/// Cancel a queued or running restore. Aborting the task drops the child
/// process (spawned with `kill_on_drop`), which terminates the worker.
#[tauri::command]
pub async fn cancel_restore(
    app: AppHandle,
    restores: State<'_, Restores>,
    id: u64,
) -> Result<(), String> {
    let mut g = restores.inner.lock().await;
    if let Some(job) = g.jobs.get_mut(&id) {
        if let Some(a) = job.abort.take() {
            a.abort();
        }
        job.view.status = "canceled".into();
        job.view.step = "canceled".into();
        let view = job.view.clone();
        drop(g);
        emit_update(&app, &view);
    }
    Ok(())
}

/// Re-run a finished, failed, or canceled job with its original parameters.
#[tauri::command]
pub async fn restart_restore(
    app: AppHandle,
    restores: State<'_, Restores>,
    id: u64,
) -> Result<(), String> {
    let inner = restores.inner.clone();
    let sem = restores.sem.clone();

    let (ipsw, ecid, mode) = {
        let mut g = inner.lock().await;
        let Some(job) = g.jobs.get_mut(&id) else {
            return Err("no such job".into());
        };
        if let Some(a) = job.abort.take() {
            a.abort();
        }
        job.view.status = "queued".into();
        job.view.step = "queued".into();
        job.view.progress = 0.0;
        job.view.message.clear();
        let view = job.view.clone();
        emit_update(&app, &view);
        (job.ipsw.clone(), job.view.ecid.clone(), job.mode.clone())
    };

    let handle = restores
        .rt
        .spawn(run_job(app, inner.clone(), sem, id, ipsw, ecid, mode));
    if let Some(job) = inner.lock().await.jobs.get_mut(&id) {
        job.abort = Some(handle.abort_handle());
    }
    Ok(())
}

/// Remove a finished (done/failed/canceled) job from the list, so its device
/// row reverts to plain device state — or disappears if the Mac is unplugged.
#[tauri::command]
pub async fn clear_restore_job(restores: State<'_, Restores>, id: u64) -> Result<(), String> {
    let mut g = restores.inner.lock().await;
    if let Some(job) = g.jobs.get(&id) {
        if matches!(job.view.status.as_str(), "queued" | "running") {
            return Err("job is still active; cancel it first".into());
        }
        g.jobs.remove(&id);
        g.order.retain(|x| *x != id);
    }
    Ok(())
}

/// A snapshot of all jobs, in enqueue order.
#[tauri::command]
pub async fn list_restore_jobs(restores: State<'_, Restores>) -> Result<Vec<JobView>, String> {
    let g = restores.inner.lock().await;
    Ok(g
        .order
        .iter()
        .filter_map(|id| g.jobs.get(id))
        .map(|j| j.view.clone())
        .collect())
}
