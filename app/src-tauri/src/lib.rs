//! MAT2 Wrapper — Rust core.
//!
//! Architecture (HANDOFF.md §10): the WebView talks to this core through a
//! narrow, typed IPC boundary. Rust owns approved paths, job state and every
//! `std::process::Command` invocation of the supplied MAT2 runtime. The
//! frontend never receives a generic execute/shell/filesystem/URL capability,
//! and absolute source paths never cross the boundary (opaque IDs only).

mod jobs;
mod log_sanitize;
mod mat2_runner;
mod model;
mod output;
mod selection;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use jobs::{FileJobResult, JobItem, JobSettings};
use mat2_runner::{Mat2Runtime, UnknownMembers};
use model::{FileStatus, PublicSelectedFile};
use serde::Deserialize;
use selection::Registry;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tauri_plugin_dialog::DialogExt;

struct ActiveJob {
    job_id: String,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    outputs: Arc<Mutex<Vec<PathBuf>>>,
}

#[derive(Default)]
struct AppState {
    registry: Registry,
    job: Mutex<Option<ActiveJob>>,
    custom_output_root: Mutex<Option<PathBuf>>,
    /// SECURITY: destructive in-place mode must be armed IN-SESSION by an
    /// explicit user action (set_inplace_armed). This flag lives only in
    /// process memory, so it is false on every launch and stale frontend
    /// state can never re-enable destructive mode silently. The arm is
    /// consumed by the first job that uses it (one confirmation per job).
    inplace_armed: AtomicBool,
}

fn register_paths(app: &AppHandle, paths: Vec<PathBuf>) {
    let state = app.state::<AppState>();
    let mut plain_files: Vec<PathBuf> = Vec::new();
    let mut folder_roots: Vec<PathBuf> = Vec::new();

    for path in paths {
        let Ok(md) = fs::symlink_metadata(&path) else {
            continue;
        };
        let ft = md.file_type();
        if ft.is_dir() {
            folder_roots.push(path);
        } else if ft.is_symlink() {
            // SECURITY INVARIANT: a symlink presented as an explicit drop/pick
            // root is resolved once via canonicalize (the user's own intent);
            // traversal inside still never follows nested symlinks.
            match fs::canonicalize(&path) {
                Ok(c) if c.is_dir() => folder_roots.push(c),
                _ => plain_files.push(path),
            }
        } else {
            plain_files.push(path);
        }
    }

    if !plain_files.is_empty() {
        state.registry.add_paths(&plain_files, None);
    }
    for root in folder_roots {
        let canonical_root = fs::canonicalize(&root).unwrap_or(root);
        let files = selection::enumerate_folder(&canonical_root);
        state.registry.add_paths(&files, Some(&canonical_root));
    }

    let _ = app.emit("selection-changed", ());
}

#[tauri::command]
fn list_selection(state: State<'_, AppState>) -> Vec<PublicSelectedFile> {
    state.registry.public_list()
}

#[tauri::command]
fn remove_items(app: AppHandle, state: State<'_, AppState>, ids: Vec<String>) -> usize {
    let removed = state.registry.remove(&ids);
    if removed > 0 {
        let _ = app.emit("selection-changed", ());
    }
    removed
}

#[tauri::command]
fn select_files(app: AppHandle, window: Window) {
    let handle = app.clone();
    app.dialog()
        .file()
        .set_parent(&window)
        .pick_files(move |picked| {
            if let Some(items) = picked {
                let paths: Vec<PathBuf> = items.into_iter().filter_map(|fp| fp.into_path().ok()).collect();
                register_paths(&handle, paths);
            }
        });
}

#[tauri::command]
fn select_folder(app: AppHandle, window: Window) {
    let handle = app.clone();
    app.dialog()
        .file()
        .set_parent(&window)
        .pick_folder(move |picked| {
            if let Some(item) = picked {
                if let Ok(path) = item.into_path() {
                    register_paths(&handle, vec![path]);
                }
            }
        });
}

#[tauri::command]
fn choose_output_root(app: AppHandle, window: Window) {
    let handle = app.clone();
    app.dialog()
        .file()
        .set_parent(&window)
        .pick_folder(move |picked| {
            if let Some(item) = picked {
                if let Ok(path) = item.into_path() {
                    if let Ok(canonical) = fs::canonicalize(&path) {
                        *handle.state::<AppState>().custom_output_root.lock().expect("poisoned") =
                            Some(canonical.clone());
                        let display = canonical
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| canonical.to_string_lossy().into_owned());
                        let _ = handle.emit("output-root-changed", serde_json::json!({ "displayName": display }));
                    }
                }
            }
        });
}

#[tauri::command]
fn output_root_info(state: State<'_, AppState>) -> Option<String> {
    state
        .custom_output_root
        .lock()
        .expect("poisoned")
        .as_ref()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned())
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobSettingsDto {
    lightweight: bool,
    verbose: bool,
    unknown_members: String,
    output: String,
    #[serde(default)]
    inplace: bool,
}

#[tauri::command]
fn set_inplace_armed(state: State<'_, AppState>, armed: bool) {
    state.inplace_armed.store(armed, Ordering::SeqCst);
}

struct TauriJobEvents {
    app: AppHandle,
    outputs: Arc<Mutex<Vec<PathBuf>>>,
}

impl jobs::JobEvents for TauriJobEvents {
    fn status(&self, id: &str, status: FileStatus) {
        self.app.state::<AppState>().registry.set_status(id, status);
        let _ = self.app.emit(
            "job-status",
            serde_json::json!({ "id": id, "status": status }),
        );
    }

    fn log(&self, line: &str) {
        let _ = self
            .app
            .emit("job-log", serde_json::json!({ "line": log_sanitize::sanitize(line) }));
    }

    fn file_result(&self, result: &FileJobResult, final_path: Option<&Path>) {
        if let Some(p) = final_path {
            self.outputs.lock().expect("poisoned").push(p.to_path_buf());
        }
        let _ = self.app.emit("job-file-result", result);
    }
}

#[tauri::command]
fn start_clean_job(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    settings: JobSettingsDto,
) -> Result<String, String> {
    {
        let slot = state.job.lock().expect("poisoned");
        if let Some(active) = slot.as_ref() {
            if !active.done.load(Ordering::SeqCst) {
                return Err("a job is already running".into());
            }
        }
    }
    if ids.is_empty() {
        return Err("no files selected".into());
    }

    let mut items: Vec<JobItem> = Vec::with_capacity(ids.len());
    for id in &ids {
        let snap = state
            .registry
            .snapshot(id)
            .ok_or_else(|| format!("unknown selection id: {id}"))?;
        items.push(JobItem {
            id: id.clone(),
            path: snap.path,
            display_name: snap.display_name,
            relative_path: snap.relative_path,
        });
    }

    let unknown_members = match settings.unknown_members.as_str() {
        "abort" => UnknownMembers::Abort,
        "omit" => UnknownMembers::Omit,
        "keep" => UnknownMembers::Keep,
        other => return Err(format!("invalid unknown-members policy: {other}")),
    };
    let custom_output_root = if settings.inplace {
        None
    } else {
        match settings.output.as_str() {
            "beside" => None,
            "custom" => Some(
                state
                    .custom_output_root
                    .lock()
                    .expect("poisoned")
                    .clone()
                    .ok_or_else(|| "custom output folder not chosen yet".to_string())?,
            ),
            other => return Err(format!("invalid output mode: {other}")),
        }
    };

    if settings.inplace {
        if !state.inplace_armed.swap(false, Ordering::SeqCst) {
            return Err(
                "destructive in-place mode is not armed for this session; enable it explicitly in Advanced settings"
                    .into(),
            );
        }
    }

    let rt = Mat2Runtime::resolve()?;
    for item in &items {
        state.registry.set_status(&item.id, FileStatus::Queued);
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let outputs = Arc::new(Mutex::new(Vec::new()));
    *state.job.lock().expect("poisoned") = Some(ActiveJob {
        job_id: job_id.clone(),
        cancel: cancel.clone(),
        done: done.clone(),
        outputs: outputs.clone(),
    });

    let job_settings = JobSettings {
        lightweight: settings.lightweight,
        verbose: settings.verbose,
        unknown_members,
        custom_output_root,
        inplace: settings.inplace,
    };
    let events = Arc::new(TauriJobEvents {
        app: app.clone(),
        outputs,
    });
    let job_id_for_thread = job_id.clone();

    std::thread::spawn(move || {
        let report = jobs::run_job(&rt, items, &job_settings, events.as_ref(), &cancel);
        for r in &report.results {
            app.state::<AppState>().registry.set_status(&r.id, r.status);
        }
        done.store(true, Ordering::SeqCst);
        let _ = app.emit(
            "job-finished",
            serde_json::json!({ "jobId": job_id_for_thread, "cancelled": report.cancelled }),
        );
    });

    Ok(job_id)
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>) -> bool {
    let slot = state.job.lock().expect("poisoned");
    if let Some(active) = slot.as_ref() {
        if !active.done.load(Ordering::SeqCst) {
            active.cancel.store(true, Ordering::SeqCst);
            return true;
        }
    }
    false
}

#[tauri::command]
fn inspect_selection(state: State<'_, AppState>, id: String) -> Result<model::InspectionDto, String> {
    let snap = state
        .registry
        .snapshot(&id)
        .ok_or_else(|| "unknown selection id".to_string())?;
    let rt = Mat2Runtime::resolve()?;
    let result = rt.inspect_json(&snap.path)?;
    Ok(result.into())
}

fn diagnostic_output(f: impl Fn(&Mat2Runtime) -> Result<mat2_runner::Mat2Output, String>) -> Result<String, String> {
    let rt = Mat2Runtime::resolve()?;
    let out = f(&rt)?;
    let mut text = out.stdout;
    if !out.stderr.trim().is_empty() {
        text.push_str("\n[stderr]\n");
        text.push_str(&out.stderr);
    }
    Ok(log_sanitize::sanitize(&text))
}

#[tauri::command]
fn mat2_version() -> Result<String, String> {
    diagnostic_output(|rt| rt.version())
}

#[tauri::command]
fn mat2_formats() -> Result<String, String> {
    diagnostic_output(|rt| rt.list_formats())
}

#[tauri::command]
fn mat2_check_dependencies() -> Result<String, String> {
    diagnostic_output(|rt| rt.check_dependencies())
}

#[tauri::command]
fn mat2_help() -> Result<String, String> {
    diagnostic_output(|rt| rt.help())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_selection,
            remove_items,
            select_files,
            select_folder,
            choose_output_root,
            output_root_info,
            start_clean_job,
            cancel_job,
            set_inplace_armed,
            inspect_selection,
            mat2_version,
            mat2_formats,
            mat2_check_dependencies,
            mat2_help
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::WindowEvent {
                event: window_event,
                ..
            } = event
            {
                match window_event {
                    tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                        register_paths(app_handle, paths);
                    }
                    tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Enter { .. }) => {
                        let _ = app_handle.emit("drag-enter", ());
                    }
                    tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Leave { .. }) => {
                        let _ = app_handle.emit("drag-leave", ());
                    }
                    _ => {}
                }
            }
        });
}
