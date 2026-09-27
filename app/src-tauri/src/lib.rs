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
use std::path::PathBuf;

use model::PublicSelectedFile;
use selection::Registry;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
struct AppState {
    registry: Registry,
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_selection,
            remove_items,
            select_files,
            select_folder
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }),
                ..
            } = event
            {
                register_paths(app_handle, paths);
            }
        });
}
