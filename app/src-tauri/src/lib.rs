/// MAT2 Wrapper — Rust core.
///
/// Architecture (HANDOFF.md §10): the WebView talks to this core through a
/// narrow, typed IPC boundary. Rust owns approved paths, job state and every
/// `std::process::Command` invocation of the supplied MAT2 runtime. The
/// frontend never receives a generic execute/shell/filesystem/URL capability.
///
/// Modules land incrementally per IMPLEMENTATION_PLAN.md:
/// - Task 4:  model, selection
/// - Task 5:  mat2_runner
/// - Task 6:  log_sanitize
/// - Task 7:  output
/// - Task 8/10/11: jobs
/// - Task 14: external
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
