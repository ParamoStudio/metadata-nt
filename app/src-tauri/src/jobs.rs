use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::log_sanitize::sanitize;
use crate::mat2_runner::{expected_cleaned_path, CleanOptions, Mat2Output, Mat2Runtime, UnknownMembers};
use crate::model::{
    diff_metadata, summarize, DiffSummary, FileStatus, MetadataDiff, MetadataEntry,
};
use crate::output::{self, OutputMode};

#[derive(Debug)]
pub enum PipelineError {
    Unsupported(String),
    Mat2Failure(String),
    OutputMissing(String),
    OutputInvalid(String),
    Commit(String),
    Io(String),
    Cancelled,
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Unsupported(m) => write!(f, "unsupported: {m}"),
            PipelineError::Mat2Failure(m) => write!(f, "MAT2 failure: {m}"),
            PipelineError::OutputMissing(m) => write!(f, "output missing: {m}"),
            PipelineError::OutputInvalid(m) => write!(f, "output invalid: {m}"),
            PipelineError::Commit(m) => write!(f, "commit failed: {m}"),
            PipelineError::Io(m) => write!(f, "I/O error: {m}"),
            PipelineError::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Private per-job staging workspace.
///
/// Note: `cleanup` is a regular recursive delete — on SSD/APFS this is NOT a
/// secure erase (docs/THREAT_MODEL.md, residual risk 2).
pub struct JobWorkspace {
    root: PathBuf,
}

impl JobWorkspace {
    pub fn create() -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("mat2-wrapper-job-{}", Uuid::new_v4()));
        fs::create_dir(&dir).map_err(|e| format!("cannot create job workspace: {e}"))?;
        apply_private_perms(&dir)?;
        Ok(Self { root: dir })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Unique private directory for staging one source file; the original
    /// file name (and thus its extension, which MAT2 dispatches on) is kept.
    pub fn stage_dir(&self) -> Result<PathBuf, String> {
        let dir = self.root.join(format!("stage-{}", Uuid::new_v4()));
        fs::create_dir(&dir).map_err(|e| format!("cannot create stage dir: {e}"))?;
        apply_private_perms(&dir)?;
        Ok(dir)
    }

    pub fn exists(&self) -> bool {
        self.root.exists()
    }

    pub fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn apply_private_perms(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(dir).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o700);
        fs::set_permissions(dir, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {:?}: {e}", path))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Copy bytes to `dst` with exclusive creation: fails if `dst` exists (file,
/// symlink or anything else) — committed outputs are never overwritten.
/// Removes the partial file on any mid-copy failure.
pub fn copy_bytes_exclusive(src: &Path, dst: &Path) -> Result<(), String> {
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)
        .map_err(|e| format!("exclusive create failed for {:?}: {e}", dst))?;
    let copy_result = (|| -> io::Result<()> {
        let mut input = fs::File::open(src)?;
        io::copy(&mut input, &mut out)?;
        out.sync_all()?;
        Ok(())
    })();
    if let Err(e) = copy_result {
        let _ = fs::remove_file(dst);
        return Err(format!("copy to {:?} failed: {e}", dst));
    }
    if let Ok(md) = fs::metadata(src) {
        let _ = fs::set_permissions(dst, md.permissions());
    }
    Ok(())
}

#[derive(Debug)]
pub struct CleanOutcome {
    pub final_path: PathBuf,
    pub mimetype: Option<String>,
    pub pre_metadata: Vec<MetadataEntry>,
    pub post_metadata: Vec<MetadataEntry>,
    pub clean_output: Mat2Output,
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipelinePhase {
    Inspecting,
    Processing,
    Verifying,
}

/// Normal-mode pipeline for ONE file (INTERFACE.md §14):
/// pre-inspect → stage byte-identical copy → MAT2 clean (never --inplace) →
/// validate output exists/regular/non-empty → post-inspect → plan collision-free
/// final path → exclusive commit. Any failure leaves no final output.
pub fn clean_one(
    rt: &Mat2Runtime,
    ws: &JobWorkspace,
    source: &Path,
    opts: &CleanOptions,
    canonical_output_root: &Path,
    relative_dir: Option<&Path>,
) -> Result<CleanOutcome, PipelineError> {
    clean_one_tracked(rt, ws, source, opts, canonical_output_root, relative_dir, &AtomicBool::new(false), &|_| Ok(()))
}

pub fn clean_one_tracked(
    rt: &Mat2Runtime,
    ws: &JobWorkspace,
    source: &Path,
    opts: &CleanOptions,
    canonical_output_root: &Path,
    relative_dir: Option<&Path>,
    cancel: &AtomicBool,
    on_phase: &dyn Fn(PipelinePhase) -> Result<(), PipelineError>,
) -> Result<CleanOutcome, PipelineError> {
    let check_cancel = |phase: PipelinePhase| -> Result<(), PipelineError> {
        if cancel.load(Ordering::SeqCst) {
            return Err(PipelineError::Cancelled);
        }
        on_phase(phase)
    };

    if opts.inplace {
        return Err(PipelineError::Io(
            "normal pipeline refuses --inplace; destructive mode has its own path".into(),
        ));
    }

    check_cancel(PipelinePhase::Inspecting)?;
    let src_md = fs::metadata(source).map_err(|_| {
        PipelineError::Io(format!(
            "source missing or unreadable: {:?}",
            source.file_name()
        ))
    })?;
    if !src_md.is_file() {
        return Err(PipelineError::Io("source is not a regular file".into()));
    }

    let pre = rt.inspect_json(source).map_err(PipelineError::Io)?;
    if let Some(err) = &pre.error {
        return Err(PipelineError::Mat2Failure(first_line(err)));
    }
    if !pre.supported {
        return Err(PipelineError::Unsupported(
            pre.mimetype.clone().unwrap_or_else(|| "unknown format".into()),
        ));
    }

    check_cancel(PipelinePhase::Processing)?;
    let stage = ws.stage_dir().map_err(PipelineError::Io)?;
    let file_name = source
        .file_name()
        .ok_or_else(|| PipelineError::Io("source has no file name".into()))?;
    let staged = stage.join(file_name);
    fs::copy(source, &staged).map_err(|e| PipelineError::Io(format!("staging copy failed: {e}")))?;

    let cleaned = match rt.clean_cancellable(&staged, opts, cancel).map_err(PipelineError::Io)? {
        crate::mat2_runner::CleanRunOutcome::Completed(out) => out,
        crate::mat2_runner::CleanRunOutcome::Cancelled => return Err(PipelineError::Cancelled),
    };
    if !cleaned.success {
        let detail = first_line(&cleaned.stdout);
        return Err(PipelineError::Mat2Failure(if detail.is_empty() {
            format!("exit code {:?}", cleaned.exit_code)
        } else {
            detail
        }));
    }

    check_cancel(PipelinePhase::Verifying)?;
    let produced = expected_cleaned_path(&staged);
    let md = fs::symlink_metadata(&produced).map_err(|_| {
        PipelineError::OutputMissing(format!("MAT2 produced no {:?}", produced.file_name()))
    })?;
    if md.file_type().is_symlink() || !md.file_type().is_file() {
        return Err(PipelineError::OutputInvalid(
            "produced output is not a regular file".into(),
        ));
    }
    if md.len() == 0 {
        return Err(PipelineError::OutputInvalid("produced output is empty".into()));
    }

    let post = rt.inspect_json(&produced).map_err(|_| {
        PipelineError::OutputInvalid("post-inspection could not complete".into())
    })?;
    if post.error.is_some() || !post.supported {
        return Err(PipelineError::OutputInvalid(
            "post-inspection could not complete".into(),
        ));
    }

    if cancel.load(Ordering::SeqCst) {
        return Err(PipelineError::Cancelled);
    }
    let cleaned_name = produced
        .file_name()
        .ok_or_else(|| PipelineError::OutputInvalid("produced output has no file name".into()))?
        .to_string_lossy()
        .into_owned();
    let final_path = output::plan_final_path(canonical_output_root, relative_dir, &cleaned_name)
        .map_err(PipelineError::Commit)?;

    copy_bytes_exclusive(&produced, &final_path).map_err(PipelineError::Commit)?;

    Ok(CleanOutcome {
        final_path,
        mimetype: pre.mimetype.clone(),
        pre_metadata: pre.entries.clone(),
        post_metadata: post.entries.clone(),
        clean_output: cleaned,
    })
}

/// Destructive in-place pipeline for ONE file: MAT2 --inplace renames the
/// cleaned file over the original. No staging, no output root. The modified
/// source is verified afterwards; success still requires the full checks.
pub fn clean_one_inplace(
    rt: &Mat2Runtime,
    source: &Path,
    opts: &CleanOptions,
    cancel: &AtomicBool,
    on_phase: &dyn Fn(PipelinePhase) -> Result<(), PipelineError>,
) -> Result<CleanOutcome, PipelineError> {
    let check_cancel = |phase: PipelinePhase| -> Result<(), PipelineError> {
        if cancel.load(Ordering::SeqCst) {
            return Err(PipelineError::Cancelled);
        }
        on_phase(phase)
    };

    if !opts.inplace {
        return Err(PipelineError::Io(
            "in-place pipeline requires opts.inplace".into(),
        ));
    }

    check_cancel(PipelinePhase::Inspecting)?;
    let src_md = fs::metadata(source).map_err(|_| {
        PipelineError::Io(format!(
            "source missing or unreadable: {:?}",
            source.file_name()
        ))
    })?;
    if !src_md.is_file() {
        return Err(PipelineError::Io("source is not a regular file".into()));
    }

    let pre = rt.inspect_json(source).map_err(PipelineError::Io)?;
    if let Some(err) = &pre.error {
        return Err(PipelineError::Mat2Failure(first_line(err)));
    }
    if !pre.supported {
        return Err(PipelineError::Unsupported(
            pre.mimetype.clone().unwrap_or_else(|| "unknown format".into()),
        ));
    }

    check_cancel(PipelinePhase::Processing)?;
    let cleaned = match rt.clean_cancellable(source, opts, cancel).map_err(PipelineError::Io)? {
        crate::mat2_runner::CleanRunOutcome::Completed(out) => out,
        crate::mat2_runner::CleanRunOutcome::Cancelled => return Err(PipelineError::Cancelled),
    };
    if !cleaned.success {
        let detail = first_line(&cleaned.stdout);
        return Err(PipelineError::Mat2Failure(if detail.is_empty() {
            format!("exit code {:?}", cleaned.exit_code)
        } else {
            detail
        }));
    }

    check_cancel(PipelinePhase::Verifying)?;
    let md = fs::symlink_metadata(source).map_err(|_| {
        PipelineError::OutputMissing("modified source disappeared".to_string())
    })?;
    if md.file_type().is_symlink() || !md.file_type().is_file() {
        return Err(PipelineError::OutputInvalid(
            "modified source is not a regular file".into(),
        ));
    }
    if md.len() == 0 {
        return Err(PipelineError::OutputInvalid("modified source is empty".into()));
    }

    let post = rt.inspect_json(source).map_err(|_| {
        PipelineError::OutputInvalid("post-inspection could not complete".into())
    })?;
    if post.error.is_some() || !post.supported {
        return Err(PipelineError::OutputInvalid(
            "post-inspection could not complete".into(),
        ));
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(PipelineError::Cancelled);
    }

    Ok(CleanOutcome {
        final_path: source.to_path_buf(),
        mimetype: pre.mimetype.clone(),
        pre_metadata: pre.entries.clone(),
        post_metadata: post.entries.clone(),
        clean_output: cleaned,
    })
}

// ---------------------------------------------------------------------------
// Job state machine (Task 10) — Tauri-independent; the command layer in
// lib.rs implements JobEvents over app.emit + the selection registry.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct JobItem {
    pub id: String,
    pub path: PathBuf,
    pub display_name: String,
    pub relative_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileJobResult {
    pub id: String,
    pub display_name: String,
    pub status: FileStatus,
    pub detail: String,
    pub diffs: Vec<MetadataDiff>,
    pub summary: Option<DiffSummary>,
    /// True when a verified output was committed (frontend may offer Reveal).
    /// The path itself never crosses IPC; reveal_output resolves it Rust-side.
    pub committed: bool,
}

pub trait JobEvents: Send + Sync {
    fn status(&self, id: &str, status: FileStatus);
    fn log(&self, line: &str);
    fn file_result(&self, result: &FileJobResult, final_path: Option<&Path>);
}

#[derive(Clone, Debug)]
pub struct JobSettings {
    pub lightweight: bool,
    pub verbose: bool,
    pub unknown_members: UnknownMembers,
    /// Some(canonical root) = Custom folder mode; None = Beside source.
    pub custom_output_root: Option<PathBuf>,
    /// Destructive mode: clean sources in place (no staging, no output root).
    /// Requires the session arm gate in lib.rs before a job may use it.
    pub inplace: bool,
}

impl Default for JobSettings {
    fn default() -> Self {
        Self {
            lightweight: false,
            verbose: false,
            unknown_members: UnknownMembers::Abort,
            custom_output_root: None,
            inplace: false,
        }
    }
}

#[derive(Debug)]
pub struct JobReport {
    pub results: Vec<FileJobResult>,
    pub cancelled: bool,
}

fn log_line(events: &dyn JobEvents, message: &str) {
    let stamp = {
        let fmt = time::macros::format_description!("[hour]:[minute]:[second]");
        time::OffsetDateTime::now_local()
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
            .format(&fmt)
            .unwrap_or_else(|_| "--:--:--".to_string())
    };
    events.log(&format!("[{stamp}] {}", sanitize(message)));
}

fn batch_root_of(path: &Path, relative: &Path) -> Option<PathBuf> {
    let depth = relative.components().count();
    path.ancestors().nth(depth).map(PathBuf::from)
}

fn relative_dir_of(relative: &Path) -> Option<PathBuf> {
    let parent = relative.parent()?;
    if parent == Path::new("") || parent == Path::new(".") {
        None
    } else {
        Some(parent.to_path_buf())
    }
}

/// Sequential batch execution (v1: no parallelism). Green Processed requires
/// the full verified pipeline; anything else is Warning/Failed/Unsupported/
/// Cancelled — never a false success.
pub fn run_job(
    rt: &Mat2Runtime,
    items: Vec<JobItem>,
    settings: &JobSettings,
    events: &dyn JobEvents,
    cancel: &AtomicBool,
) -> JobReport {
    let ws = if settings.inplace {
        None
    } else {
        match JobWorkspace::create() {
            Ok(ws) => Some(ws),
            Err(e) => {
                let results: Vec<FileJobResult> = items
                    .iter()
                    .map(|it| FileJobResult {
                        id: it.id.clone(),
                        display_name: it.display_name.clone(),
                        status: FileStatus::Failed,
                        detail: sanitize(&e),
                        diffs: Vec::new(),
                        summary: None,
                        committed: false,
                    })
                    .collect();
                for r in &results {
                    events.status(&r.id, FileStatus::Failed);
                }
                return JobReport { results, cancelled: false };
            }
        }
    };

    let timestamp = output::timestamp_now();
    let opts = CleanOptions {
        lightweight: settings.lightweight,
        verbose: settings.verbose,
        unknown_members: settings.unknown_members,
        inplace: settings.inplace,
    };
    let mode = match &settings.custom_output_root {
        Some(root) => OutputMode::Custom(root.clone()),
        None => OutputMode::BesideSource,
    };

    let mut results = Vec::new();
    let mut roots: HashMap<PathBuf, Result<PathBuf, String>> = HashMap::new();

    for item in &items {
        events.status(&item.id, FileStatus::Queued);
    }

    for item in &items {
        if cancel.load(Ordering::SeqCst) {
            let r = FileJobResult {
                id: item.id.clone(),
                display_name: item.display_name.clone(),
                status: FileStatus::Cancelled,
                detail: "Cancelled before processing".into(),
                diffs: Vec::new(),
                summary: None,
                committed: false,
            };
            events.status(&item.id, FileStatus::Cancelled);
            events.file_result(&r, None);
            results.push(r);
            continue;
        }

        log_line(events, &format!("Queued {}", item.display_name));

        let id_for_events = item.id.clone();
        let on_phase = |phase: PipelinePhase| -> Result<(), PipelineError> {
            let st = match phase {
                PipelinePhase::Inspecting => FileStatus::Inspecting,
                PipelinePhase::Processing => FileStatus::Processing,
                PipelinePhase::Verifying => FileStatus::Verifying,
            };
            events.status(&id_for_events, st);
            match phase {
                PipelinePhase::Inspecting => log_line(events, &format!("Inspecting source of {} with MAT2", item.display_name)),
                PipelinePhase::Processing => {
                    let msg = if settings.inplace {
                        format!("Running MAT2 IN PLACE on {} (original will be modified)", item.display_name)
                    } else {
                        format!("Creating private staging copy and running MAT2 on {}", item.display_name)
                    };
                    log_line(events, &msg)
                }
                PipelinePhase::Verifying => log_line(events, &format!("Verifying output of {} with MAT2", item.display_name)),
            }
            Ok(())
        };

        let outcome = if settings.inplace {
            clean_one_inplace(rt, &item.path, &opts, cancel, &on_phase)
        } else {
            let root_base = match (&item.relative_path, &mode) {
                (_, OutputMode::Custom(root)) => root.clone(),
                (Some(rel), OutputMode::BesideSource) => {
                    batch_root_of(&item.path, rel).unwrap_or_else(|| {
                        item.path.parent().map(PathBuf::from).unwrap_or_else(|| item.path.clone())
                    })
                }
                (None, OutputMode::BesideSource) => item
                    .path
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| item.path.clone()),
            };

            let canonical_root = match roots.entry(root_base.clone()).or_insert_with(|| {
                output::job_root(&mode, &root_base, &timestamp)
                    .and_then(|r| output::ensure_root(&r))
            }) {
                Ok(r) => r.clone(),
                Err(e) => {
                    let r = FileJobResult {
                        id: item.id.clone(),
                        display_name: item.display_name.clone(),
                        status: FileStatus::Failed,
                        detail: sanitize(&format!("cannot prepare output location: {e}")),
                        diffs: Vec::new(),
                        summary: None,
                        committed: false,
                    };
                    log_line(events, &format!("{}: {}", item.display_name, r.detail));
                    events.status(&item.id, FileStatus::Failed);
                    events.file_result(&r, None);
                    results.push(r);
                    continue;
                }
            };

            let relative_dir = item
                .relative_path
                .as_deref()
                .and_then(relative_dir_of);

            clean_one_tracked(
                rt,
                ws.as_ref().expect("workspace exists in normal mode"),
                &item.path,
                &opts,
                &canonical_root,
                relative_dir.as_deref(),
                cancel,
                &on_phase,
            )
        };

        let result = match outcome {
            Ok(o) => {
                if settings.verbose {
                    for line in o.clean_output.stderr.lines().chain(o.clean_output.stdout.lines()) {
                        if !line.trim().is_empty() {
                            log_line(events, &format!("mat2: {line}"));
                        }
                    }
                }
                let diffs = diff_metadata(&o.pre_metadata, &o.post_metadata);
                let summary = summarize(&diffs, o.pre_metadata.len());
                if settings.inplace {
                    log_line(events, &format!("MAT2 modified {} in place", item.display_name));
                } else {
                    log_line(events, &format!("MAT2 output created for {}", item.display_name));
                    log_line(events, &format!("Committed output for {}", item.display_name));
                }
                let (status, detail) = if o.post_metadata.is_empty() {
                    log_line(events, &format!("Result: 0 metadata fields detectable by MAT2 in {}", item.display_name));
                    (FileStatus::Processed, "No metadata detectable by MAT2".to_string())
                } else {
                    let n = o.post_metadata.len();
                    log_line(events, &format!("Result: {n} metadata fields still detectable by MAT2 in {}", item.display_name));
                    (FileStatus::Warning, format!("MAT2 still detects {n} metadata fields"))
                };
                let r = FileJobResult {
                    id: item.id.clone(),
                    display_name: item.display_name.clone(),
                    status,
                    detail,
                    diffs,
                    summary: Some(summary),
                    committed: true,
                };
                events.status(&item.id, status);
                events.file_result(&r, Some(&o.final_path));
                results.push(r);
                continue;
            }
            Err(PipelineError::Cancelled) => FileJobResult {
                id: item.id.clone(),
                display_name: item.display_name.clone(),
                status: FileStatus::Cancelled,
                detail: "Cancelled during processing".into(),
                diffs: Vec::new(),
                summary: None,
                committed: false,
            },
            Err(PipelineError::Unsupported(m)) => FileJobResult {
                id: item.id.clone(),
                display_name: item.display_name.clone(),
                status: FileStatus::Unsupported,
                detail: sanitize(&format!("MAT2 does not support this format ({m}); not processed")),
                diffs: Vec::new(),
                summary: None,
                committed: false,
            },
            Err(e) => FileJobResult {
                id: item.id.clone(),
                display_name: item.display_name.clone(),
                status: FileStatus::Failed,
                detail: sanitize(&e.to_string()),
                diffs: Vec::new(),
                summary: None,
                committed: false,
            },
        };
        log_line(events, &format!("{}: {}", result.display_name, result.detail));
        events.status(&item.id, result.status);
        events.file_result(&result, None);
        results.push(result);
    }

    if let Some(ws) = &ws {
        ws.cleanup();
    }
    JobReport {
        cancelled: cancel.load(Ordering::SeqCst),
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mat2_runner::Mat2Runtime;
    use crate::output::{ensure_root, job_root};
    use std::sync::Mutex;

    fn runtime_or_skip() -> Option<Mat2Runtime> {
        match Mat2Runtime::resolve() {
            Ok(rt) => Some(rt),
            Err(e) => {
                eprintln!("SKIP integration: {e}");
                None
            }
        }
    }

    fn project_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap()
    }

    fn tempdir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("mat2job-{}-{}-{}", tag, std::process::id(), nanos));
        fs::create_dir_all(&p).unwrap();
        fs::canonicalize(&p).unwrap()
    }

    fn fixture(name: &str) -> PathBuf {
        project_root().join("upstream-mat2/tests/data").join(name)
    }

    #[derive(Default)]
    struct MockEvents {
        records: Mutex<Vec<Recorded>>,
        on_result: Mutex<Option<Box<dyn Fn(&FileJobResult) + Send>>>,
        on_status: Mutex<Option<Box<dyn Fn(&str, FileStatus) + Send>>>,
    }

    #[derive(Clone, Debug)]
    enum Recorded {
        Status(String, FileStatus),
        Log(String),
        Result(FileJobResult),
    }

    impl MockEvents {
        fn statuses(&self) -> Vec<(String, FileStatus)> {
            self.records
                .lock()
                .unwrap()
                .iter()
                .filter_map(|r| match r {
                    Recorded::Status(id, s) => Some((id.clone(), *s)),
                    _ => None,
                })
                .collect()
        }
        fn results(&self) -> Vec<FileJobResult> {
            self.records
                .lock()
                .unwrap()
                .iter()
                .filter_map(|r| match r {
                    Recorded::Result(res) => Some(res.clone()),
                    _ => None,
                })
                .collect()
        }
        fn logs(&self) -> Vec<String> {
            self.records
                .lock()
                .unwrap()
                .iter()
                .filter_map(|r| match r {
                    Recorded::Log(l) => Some(l.clone()),
                    _ => None,
                })
                .collect()
        }
        fn set_on_result(&self, f: impl Fn(&FileJobResult) + Send + 'static) {
            *self.on_result.lock().unwrap() = Some(Box::new(f));
        }
        fn set_on_status(&self, f: impl Fn(&str, FileStatus) + Send + 'static) {
            *self.on_status.lock().unwrap() = Some(Box::new(f));
        }
    }

    impl JobEvents for MockEvents {
        fn status(&self, id: &str, status: FileStatus) {
            if let Some(f) = self.on_status.lock().unwrap().as_ref() {
                f(id, status);
            }
            self.records.lock().unwrap().push(Recorded::Status(id.to_string(), status));
        }
        fn log(&self, line: &str) {
            self.records.lock().unwrap().push(Recorded::Log(line.to_string()));
        }
        fn file_result(&self, result: &FileJobResult, _final_path: Option<&Path>) {
            if let Some(f) = self.on_result.lock().unwrap().as_ref() {
                f(result);
            }
            self.records.lock().unwrap().push(Recorded::Result(result.clone()));
        }
    }

    fn item(id: &str, path: PathBuf) -> JobItem {
        let display_name = path.file_name().unwrap().to_string_lossy().into_owned();
        JobItem { id: id.to_string(), path, display_name, relative_path: None }
    }

    #[test]
    fn staging_is_byte_identical() {
        let ws = JobWorkspace::create().unwrap();
        let src = fixture("dirty.jpg");
        let stage = ws.stage_dir().unwrap();
        let staged = stage.join("dirty.jpg");
        fs::copy(&src, &staged).unwrap();
        assert_eq!(sha256_file(&src).unwrap(), sha256_file(&staged).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(ws.root()).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "workspace must be private");
        }
        ws.cleanup();
        assert!(!ws.exists());
    }

    #[test]
    fn copy_bytes_exclusive_never_overwrites() {
        let dir = tempdir("exclusive");
        let src = dir.join("src.bin");
        let dst = dir.join("dst.bin");
        fs::write(&src, b"new bytes").unwrap();
        fs::write(&dst, b"ORIGINAL").unwrap();
        let err = copy_bytes_exclusive(&src, &dst);
        assert!(err.is_err());
        assert_eq!(fs::read(&dst).unwrap(), b"ORIGINAL");

        std::os::unix::fs::symlink(&src, dir.join("link.bin")).unwrap();
        assert!(copy_bytes_exclusive(&src, &dir.join("link.bin")).is_err());

        let fresh = dir.join("fresh.bin");
        copy_bytes_exclusive(&src, &fresh).unwrap();
        assert_eq!(fs::read(&fresh).unwrap(), b"new bytes");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn full_pipeline_cleans_verifies_commits_and_preserves_source() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("fullpipeline");
        let src = dir.join("dirty.jpg");
        fs::copy(fixture("dirty.jpg"), &src).unwrap();
        let before = sha256_file(&src).unwrap();

        let ws = JobWorkspace::create().unwrap();
        let root = job_root(&OutputMode::BesideSource, &dir, &output::timestamp_now()).unwrap();
        let canonical = ensure_root(&root).unwrap();
        let outcome = clean_one(&rt, &ws, &src, &CleanOptions::default(), &canonical, None).unwrap();

        assert!(outcome.final_path.starts_with(&canonical));
        assert_eq!(outcome.final_path.file_name().unwrap(), "dirty.cleaned.jpg");
        assert!(fs::metadata(&outcome.final_path).unwrap().len() > 0);
        assert!(outcome.post_metadata.is_empty(), "post: {:?}", outcome.post_metadata);
        assert!(outcome
            .pre_metadata
            .iter()
            .any(|e| e.key == "Comment" && e.display_value == "Created with GIMP"));
        assert_eq!(sha256_file(&src).unwrap(), before, "source must be unchanged");

        ws.cleanup();
        assert!(!ws.exists(), "workspace must be removed");
        assert!(outcome.final_path.exists(), "committed output survives workspace cleanup");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unsupported_input_leaves_no_output() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("unsupported");
        let weird = dir.join("notes.unknownext123");
        fs::write(&weird, b"hello").unwrap();

        let ws = JobWorkspace::create().unwrap();
        let root = job_root(&OutputMode::BesideSource, &dir, &output::timestamp_now()).unwrap();
        let canonical = ensure_root(&root).unwrap();
        let err = clean_one(&rt, &ws, &weird, &CleanOptions::default(), &canonical, None);
        assert!(matches!(err, Err(PipelineError::Unsupported(_))), "got {:?}", err);

        let leftovers: Vec<_> = walkdir_flat(&canonical);
        assert!(leftovers.is_empty(), "no final output may exist: {:?}", leftovers);
        assert!(!dir.join("notes.unknownext123.cleaned").exists());
        ws.cleanup();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clean_failure_leaves_no_final_output() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("cleanfail");
        let fake = dir.join("corrupt.jpg");
        fs::write(&fake, b"this is not a jpeg at all").unwrap();

        let ws = JobWorkspace::create().unwrap();
        let root = job_root(&OutputMode::BesideSource, &dir, &output::timestamp_now()).unwrap();
        let canonical = ensure_root(&root).unwrap();
        let err = clean_one(&rt, &ws, &fake, &CleanOptions::default(), &canonical, None);
        assert!(err.is_err(), "corrupt file must fail");
        let leftovers = walkdir_flat(&canonical);
        assert!(leftovers.is_empty(), "no output on failure: {:?}", leftovers);
        ws.cleanup();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn second_run_collides_without_overwriting() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("collisionpipe");
        let src = dir.join("dirty.png");
        fs::copy(fixture("dirty.png"), &src).unwrap();

        let ws = JobWorkspace::create().unwrap();
        let root = job_root(&OutputMode::BesideSource, &dir, "2026-01-01_000000").unwrap();
        let canonical = ensure_root(&root).unwrap();

        let first = clean_one(&rt, &ws, &src, &CleanOptions::default(), &canonical, None).unwrap();
        let first_bytes = fs::read(&first.final_path).unwrap();
        let second = clean_one(&rt, &ws, &src, &CleanOptions::default(), &canonical, None).unwrap();

        assert_eq!(first.final_path.file_name().unwrap(), "dirty.cleaned.png");
        assert_eq!(second.final_path.file_name().unwrap(), "dirty.cleaned-2.png");
        assert_eq!(fs::read(&first.final_path).unwrap(), first_bytes, "first output untouched");
        ws.cleanup();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancel_after_commit_keeps_committed_drops_staged() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("cancelpipe");
        let a = dir.join("dirty.jpg");
        let b = dir.join("dirty.png");
        fs::copy(fixture("dirty.jpg"), &a).unwrap();
        fs::copy(fixture("dirty.png"), &b).unwrap();

        let ws = JobWorkspace::create().unwrap();
        let root = job_root(&OutputMode::BesideSource, &dir, "2026-01-01_000001").unwrap();
        let canonical = ensure_root(&root).unwrap();

        let committed = clean_one(&rt, &ws, &a, &CleanOptions::default(), &canonical, None).unwrap();

        let stage = ws.stage_dir().unwrap();
        let staged_b = stage.join("dirty.png");
        fs::copy(&b, &staged_b).unwrap();
        ws.cleanup();

        assert!(committed.final_path.exists(), "committed output survives cancellation");
        assert!(!ws.exists(), "workspace including staged file removed");
        let leftovers = walkdir_flat(&canonical);
        assert_eq!(leftovers.len(), 1, "only the committed file may exist: {:?}", leftovers);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn normal_pipeline_refuses_inplace_option() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("noinplace");
        let src = dir.join("dirty.jpg");
        fs::copy(fixture("dirty.jpg"), &src).unwrap();
        let ws = JobWorkspace::create().unwrap();
        let root = job_root(&OutputMode::BesideSource, &dir, &output::timestamp_now()).unwrap();
        let canonical = ensure_root(&root).unwrap();
        let opts = CleanOptions { inplace: true, ..Default::default() };
        let err = clean_one(&rt, &ws, &src, &opts, &canonical, None);
        assert!(matches!(err, Err(PipelineError::Io(_))));
        ws.cleanup();
        fs::remove_dir_all(&dir).unwrap();
    }

    // ---------------- state machine (Task 10) ----------------

    #[test]
    fn run_job_happy_path_sequential_with_full_status_ladder() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("runjob-happy");
        let a = dir.join("one.jpg");
        let b = dir.join("two.png");
        fs::copy(fixture("dirty.jpg"), &a).unwrap();
        fs::copy(fixture("dirty.png"), &b).unwrap();
        let sha_a = sha256_file(&a).unwrap();

        let events = MockEvents::default();
        let cancel = AtomicBool::new(false);
        let report = run_job(&rt, vec![item("id-a", a.clone()), item("id-b", b.clone())], &JobSettings::default(), &events, &cancel);

        assert!(!report.cancelled);
        assert_eq!(report.results.len(), 2);
        assert!(report.results.iter().all(|r| r.status == FileStatus::Processed), "{:?}", report.results);

        // strict sequential ladder: file A reaches terminal status before B starts inspecting
        let st = events.statuses();
        let pos = |id: &str, s: FileStatus| st.iter().position(|(i, x)| i == id && *x == s).expect("missing transition");
        assert!(pos("id-a", FileStatus::Processed) < pos("id-b", FileStatus::Inspecting));
        for id in ["id-a", "id-b"] {
            assert!(pos(id, FileStatus::Queued) < pos(id, FileStatus::Inspecting));
            assert!(pos(id, FileStatus::Inspecting) < pos(id, FileStatus::Processing));
            assert!(pos(id, FileStatus::Processing) < pos(id, FileStatus::Verifying));
            assert!(pos(id, FileStatus::Verifying) < pos(id, FileStatus::Processed));
        }

        // diff content: jpg comment removed
        let res_a = report.results.iter().find(|r| r.id == "id-a").unwrap();
        assert!(res_a.diffs.iter().any(|d| d.key == "Comment" && d.status == crate::model::DiffStatus::Removed));
        assert_eq!(res_a.summary.unwrap().still_detectable, 0);

        // outputs inside <dir>/MAT2 Output/<ts>/, sources untouched
        let out_root = dir.join(crate::output::OUTPUT_DIR_NAME);
        let files = walkdir_flat(&out_root);
        assert_eq!(files.len(), 2, "{:?}", files);
        assert_eq!(sha256_file(&a).unwrap(), sha_a);

        // no ANSI/control chars in any log line; logs mention both files
        for l in events.logs() {
            assert!(!l.chars().any(|c| (c as u32) < 0x20 && c != '\n' && c != '\t'));
        }
        assert!(events.logs().iter().any(|l| l.contains("Committed output for one.jpg")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_mixed_unsupported_reports_per_file_status() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("runjob-mixed");
        let good = dir.join("ok.jpg");
        let weird = dir.join("notes.unknownext123");
        fs::copy(fixture("dirty.jpg"), &good).unwrap();
        fs::write(&weird, b"hello").unwrap();

        let events = MockEvents::default();
        let report = run_job(&rt, vec![item("g", good.clone()), item("w", weird.clone())], &JobSettings::default(), &events, &AtomicBool::new(false));

        let g = report.results.iter().find(|r| r.id == "g").unwrap();
        let w = report.results.iter().find(|r| r.id == "w").unwrap();
        assert_eq!(g.status, FileStatus::Processed);
        assert_eq!(w.status, FileStatus::Unsupported);
        assert!(w.detail.contains("does not support"), "{}", w.detail);
        let outputs = walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME));
        assert_eq!(outputs.len(), 1, "{:?}", outputs);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_warns_when_metadata_remains() {
        let Some(rt) = runtime_or_skip() else { return };
        if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("SKIP: ffmpeg unavailable for mp4 warning case");
            return;
        }
        let dir = tempdir("runjob-warn");
        let vid = dir.join("clip.mp4");
        fs::copy(fixture("dirty.mp4"), &vid).unwrap();

        let events = MockEvents::default();
        let report = run_job(&rt, vec![item("v", vid)], &JobSettings::default(), &events, &AtomicBool::new(false));
        let v = &report.results[0];
        assert_eq!(v.status, FileStatus::Warning, "{:?}", v);
        assert!(v.summary.unwrap().still_detectable > 0);
        assert!(v.detail.contains("still detects"), "{}", v.detail);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_cancel_before_start_cancels_everything() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("runjob-cancel0");
        let a = dir.join("a.jpg");
        fs::copy(fixture("dirty.jpg"), &a).unwrap();

        let events = MockEvents::default();
        let cancel = AtomicBool::new(true);
        let report = run_job(&rt, vec![item("a", a.clone())], &JobSettings::default(), &events, &cancel);
        assert!(report.cancelled);
        assert_eq!(report.results[0].status, FileStatus::Cancelled);
        assert!(!dir.join(crate::output::OUTPUT_DIR_NAME).join("").exists()
            || walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME)).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_cancel_during_keeps_committed_and_cancels_rest() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("runjob-cancel1");
        let a = dir.join("a.jpg");
        let b = dir.join("b.png");
        fs::copy(fixture("dirty.jpg"), &a).unwrap();
        fs::copy(fixture("dirty.png"), &b).unwrap();

        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let events = MockEvents::default();
        let cancel_for_cb = cancel.clone();
        events.set_on_result(move |r| {
            if r.id == "a" {
                cancel_for_cb.store(true, Ordering::SeqCst);
            }
        });

        let report = run_job(&rt, vec![item("a", a), item("b", b)], &JobSettings::default(), &events, &cancel);
        assert!(report.cancelled);
        let ra = report.results.iter().find(|r| r.id == "a").unwrap();
        let rb = report.results.iter().find(|r| r.id == "b").unwrap();
        assert_eq!(ra.status, FileStatus::Processed);
        assert_eq!(rb.status, FileStatus::Cancelled);
        let outputs = walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME));
        assert_eq!(outputs.len(), 1, "committed output survives: {:?}", outputs);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_deleted_after_selection_fails_visibly() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("runjob-deleted");
        let a = dir.join("gone.jpg");
        fs::copy(fixture("dirty.jpg"), &a).unwrap();
        let items = vec![item("a", a.clone())];
        fs::remove_file(&a).unwrap();

        let events = MockEvents::default();
        let report = run_job(&rt, items, &JobSettings::default(), &events, &AtomicBool::new(false));
        assert_eq!(report.results[0].status, FileStatus::Failed);
        assert!(report.results[0].detail.to_lowercase().contains("missing")
            || report.results[0].detail.to_lowercase().contains("unreadable"), "{}", report.results[0].detail);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_permission_error_fails_visibly() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("runjob-perm");
        let a = dir.join("locked.jpg");
        fs::copy(fixture("dirty.jpg"), &a).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&a).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&a, perms).unwrap();

        let events = MockEvents::default();
        let report = run_job(&rt, vec![item("a", a.clone())], &JobSettings::default(), &events, &AtomicBool::new(false));
        assert_eq!(report.results[0].status, FileStatus::Failed, "{:?}", report.results[0]);

        let mut perms = fs::metadata(&a).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&a, perms).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_corrupt_file_never_reports_success() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("runjob-corrupt");
        let a = dir.join("corrupt.jpg");
        fs::write(&a, b"definitely not a jpeg").unwrap();

        let events = MockEvents::default();
        let report = run_job(&rt, vec![item("a", a)], &JobSettings::default(), &events, &AtomicBool::new(false));
        let r = &report.results[0];
        assert_eq!(r.status, FileStatus::Failed);
        assert_ne!(r.status, FileStatus::Processed);
        assert!(walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME)).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_custom_root_collects_outputs() {
        let Some(rt) = runtime_or_skip() else { return };
        let src_dir = tempdir("runjob-custom-src");
        let out_dir = tempdir("runjob-custom-out");
        let a = src_dir.join("a.jpg");
        fs::copy(fixture("dirty.jpg"), &a).unwrap();

        let settings = JobSettings { custom_output_root: Some(out_dir.clone()), ..Default::default() };
        let events = MockEvents::default();
        let report = run_job(&rt, vec![item("a", a)], &settings, &events, &AtomicBool::new(false));
        assert_eq!(report.results[0].status, FileStatus::Processed);
        assert!(!src_dir.join(crate::output::OUTPUT_DIR_NAME).exists(), "nothing beside source in custom mode");
        let outputs = walkdir_flat(&out_dir);
        assert_eq!(outputs.len(), 1, "{:?}", outputs);
        assert!(outputs[0].starts_with(&out_dir));
        fs::remove_dir_all(&src_dir).unwrap();
        fs::remove_dir_all(&out_dir).unwrap();
    }

    #[test]
    fn run_job_folder_batch_preserves_relative_structure() {
        let Some(rt) = runtime_or_skip() else { return };
        let batch = tempdir("runjob-batch");
        fs::create_dir_all(batch.join("photos/2026")).unwrap();
        let top = batch.join("top.jpg");
        let nested = batch.join("photos/2026/deep.png");
        fs::copy(fixture("dirty.jpg"), &top).unwrap();
        fs::copy(fixture("dirty.png"), &nested).unwrap();

        let items = vec![
            JobItem {
                id: "t".into(),
                path: top.clone(),
                display_name: "top.jpg".into(),
                relative_path: Some(PathBuf::from("top.jpg")),
            },
            JobItem {
                id: "n".into(),
                path: nested.clone(),
                display_name: "deep.png".into(),
                relative_path: Some(PathBuf::from("photos/2026/deep.png")),
            },
        ];
        let events = MockEvents::default();
        let report = run_job(&rt, items, &JobSettings::default(), &events, &AtomicBool::new(false));
        assert!(report.results.iter().all(|r| r.status == FileStatus::Processed), "{:?}", report.results);

        let out_root = batch.join(crate::output::OUTPUT_DIR_NAME);
        let outputs = walkdir_flat(&out_root);
        assert_eq!(outputs.len(), 2, "{:?}", outputs);
        assert!(outputs.iter().any(|p| p.ends_with("photos/2026/deep.cleaned.png")), "{:?}", outputs);
        assert!(outputs.iter().any(|p| p.file_name().unwrap() == "top.cleaned.jpg"), "{:?}", outputs);
        fs::remove_dir_all(&batch).unwrap();
    }

    #[test]
    fn cancel_during_clean_kills_child_and_keeps_committed() {
        let Some(rt) = runtime_or_skip() else { return };
        if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("SKIP: ffmpeg unavailable for mp4 cancel window");
            return;
        }
        let dir = tempdir("runjob-cancelchild");
        let j = dir.join("j.jpg");
        let m = dir.join("m.mp4");
        let p = dir.join("p.png");
        fs::copy(fixture("dirty.jpg"), &j).unwrap();
        fs::copy(fixture("dirty.mp4"), &m).unwrap();
        fs::copy(fixture("dirty.png"), &p).unwrap();

        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let events = MockEvents::default();
        let cancel_for_cb = cancel.clone();
        // deterministic window: fire cancel exactly when the slow mp4 enters Processing
        events.set_on_status(move |id, st| {
            if id == "m" && st == FileStatus::Processing {
                cancel_for_cb.store(true, Ordering::SeqCst);
            }
        });

        let started = std::time::Instant::now();
        let report = run_job(
            &rt,
            vec![item("j", j), item("m", m), item("p", p)],
            &JobSettings::default(),
            &events,
            &cancel,
        );
        let elapsed = started.elapsed();

        assert!(report.cancelled);
        let get = |id: &str| report.results.iter().find(|r| r.id == id).unwrap();
        assert_eq!(get("j").status, FileStatus::Processed, "earlier committed file survives");
        assert_eq!(get("m").status, FileStatus::Cancelled, "in-flight file cancelled");
        assert_eq!(get("p").status, FileStatus::Cancelled, "later file never starts");

        // p must never have entered the pipeline
        let st = events.statuses();
        assert!(!st.iter().any(|(id, s)| id == "p" && *s == FileStatus::Inspecting));

        // only j's output committed
        let outputs = walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME));
        assert_eq!(outputs.len(), 1, "{:?}", outputs);
        assert!(outputs[0].file_name().unwrap().to_string_lossy().contains("j.cleaned"));

        // child termination was prompt, not a full mp4 clean
        assert!(elapsed.as_secs() < 30, "cancel should short-circuit: {elapsed:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clean_cancellable_with_preset_cancel_returns_immediately() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("cancellable-preset");
        let staged = dir.join("clip.mp4");
        fs::copy(fixture("dirty.mp4"), &staged).unwrap();

        let cancel = AtomicBool::new(true);
        let started = std::time::Instant::now();
        let outcome = rt.clean_cancellable(&staged, &CleanOptions::default(), &cancel).unwrap();
        assert!(matches!(outcome, crate::mat2_runner::CleanRunOutcome::Cancelled));
        assert!(started.elapsed().as_secs() < 5, "must return promptly");
        assert!(!crate::mat2_runner::expected_cleaned_path(&staged).exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_inplace_modifies_source_and_leaves_no_output_dir() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("inplace-basic");
        let src = dir.join("dirty.jpg");
        fs::copy(fixture("dirty.jpg"), &src).unwrap();
        let before = sha256_file(&src).unwrap();

        let events = MockEvents::default();
        let settings = JobSettings { inplace: true, ..Default::default() };
        let report = run_job(&rt, vec![item("a", src.clone())], &settings, &events, &AtomicBool::new(false));

        let r = &report.results[0];
        assert_eq!(r.status, FileStatus::Processed, "{:?}", r);
        assert!(r.diffs.iter().any(|d| d.key == "Comment"
            && d.status == crate::model::DiffStatus::Removed));
        assert_ne!(sha256_file(&src).unwrap(), before, "source must be modified in place");
        assert!(src.exists(), "source path must still exist");
        assert!(!dir.join(crate::output::OUTPUT_DIR_NAME).exists(), "in-place creates no output dir");
        let stray: Vec<_> = walkdir_flat(&dir)
            .into_iter()
            .filter(|p| p != &src)
            .collect();
        assert!(stray.is_empty(), "no extra files beside the source: {:?}", stray);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_inplace_unsupported_leaves_source_untouched() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("inplace-unsupported");
        let weird = dir.join("notes.unknownext123");
        fs::write(&weird, b"hello").unwrap();
        let before = sha256_file(&weird).unwrap();

        let events = MockEvents::default();
        let settings = JobSettings { inplace: true, ..Default::default() };
        let report = run_job(&rt, vec![item("w", weird.clone())], &settings, &events, &AtomicBool::new(false));

        assert_eq!(report.results[0].status, FileStatus::Unsupported);
        assert_eq!(sha256_file(&weird).unwrap(), before, "unsupported source must be untouched");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_job_inplace_cancel_before_start_leaves_source_untouched() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("inplace-cancel");
        let src = dir.join("dirty.jpg");
        fs::copy(fixture("dirty.jpg"), &src).unwrap();
        let before = sha256_file(&src).unwrap();

        let events = MockEvents::default();
        let settings = JobSettings { inplace: true, ..Default::default() };
        let cancel = AtomicBool::new(true);
        let report = run_job(&rt, vec![item("a", src.clone())], &settings, &events, &cancel);

        assert_eq!(report.results[0].status, FileStatus::Cancelled);
        assert_eq!(sha256_file(&src).unwrap(), before, "cancelled source must be untouched");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clean_one_inplace_requires_inplace_option() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("inplace-guard");
        let src = dir.join("dirty.jpg");
        fs::copy(fixture("dirty.jpg"), &src).unwrap();
        let err = clean_one_inplace(&rt, &src, &CleanOptions::default(), &AtomicBool::new(false), &|_| Ok(()));
        assert!(matches!(err, Err(PipelineError::Io(_))));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn security_regression_hostile_filenames_end_to_end() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("sec-hostile");
        let hostile_img_names = [
            "--version.jpg",
            "--help.png",
            "; touch owned.jpg",
            "$(touch owned2).jpg",
            "<img src=x onerror=alert(1)>.jpg",
            "quote'and\"double.png",
            "ansi\u{1b}[31mred.jpg",
        ];        let mut items = Vec::new();
        for (i, name) in hostile_img_names.iter().enumerate() {
            let fixture_src = if name.ends_with(".png") { fixture("dirty.png") } else { fixture("dirty.jpg") };
            let p = dir.join(name);
            fs::copy(&fixture_src, &p).unwrap();
            items.push(item(&format!("h{i}"), p));
        }
        // POSIX filenames cannot contain '/', so a literal `</script>` closing
        // tag is unrealizable as a name; this variant keeps the hostile HTML
        // shape (angle brackets, event handler, tag-like syntax) as data.
        let script_pdf = dir.join("<script>alert(1)<\\script>.pdf");
        fs::copy(fixture("dirty.pdf"), &script_pdf).unwrap();
        items.push(item("hpdf", script_pdf));

        let events = MockEvents::default();
        let report = run_job(&rt, items, &JobSettings::default(), &events, &AtomicBool::new(false));

        for r in &report.results {
            assert!(
                r.status == FileStatus::Processed || r.status == FileStatus::Warning,
                "hostile name {:?} should process as data, got {:?}: {}",
                r.display_name, r.status, r.detail
            );
        }
        assert!(report.results.iter().any(|r| r.display_name == "--version.jpg"));
        assert!(report.results.iter().any(|r| r.display_name == "<img src=x onerror=alert(1)>.jpg"));
        assert!(report.results.iter().any(|r| r.display_name == "<script>alert(1)<\\script>.pdf"));

        // shell-injection evidence files must NOT exist anywhere
        for probe in ["owned", "owned2"] {
            assert!(!dir.join(probe).exists(), "injection executed: {probe} created");
        }
        assert!(!std::env::current_dir().unwrap().join("owned").exists());
        assert!(!std::env::current_dir().unwrap().join("owned2").exists());

        // logs contain no ESC/control characters despite the ANSI filename
        for l in events.logs() {
            assert!(!l.chars().any(|c| (c as u32) < 0x20 && c != '\n' && c != '\t'), "control char in log: {:?}", l);
        }

        // outputs confined to the job output root
        let outputs = walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME));
        assert_eq!(outputs.len(), hostile_img_names.len() + 1, "{:?}", outputs);
        for o in &outputs {
            assert!(o.starts_with(dir.join(crate::output::OUTPUT_DIR_NAME)));
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn security_regression_symlink_source_stays_data_and_target_untouched() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("sec-symlink");
        let target = dir.join("real.jpg");
        fs::copy(fixture("dirty.jpg"), &target).unwrap();
        let target_sha = sha256_file(&target).unwrap();
        let link = dir.join("link.jpg");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let events = MockEvents::default();
        let report = run_job(&rt, vec![item("l", link.clone())], &JobSettings::default(), &events, &AtomicBool::new(false));

        let r = &report.results[0];
        assert_eq!(r.status, FileStatus::Processed, "{:?}", r);
        assert_eq!(sha256_file(&target).unwrap(), target_sha, "symlink target must be untouched");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink(), "link itself must remain a link");
        let outputs = walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME));
        assert_eq!(outputs.len(), 1, "{:?}", outputs);
        fs::remove_dir_all(&dir).unwrap();
    }

    fn tool_available(name: &str) -> bool {
        std::process::Command::new(name)
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    }

    #[test]
    fn integration_format_matrix_real_mat2() {
        let Some(rt) = runtime_or_skip() else { return };
        let has_ffmpeg = tool_available("ffmpeg");
        let has_zip = tool_available("zip");
        let has_tar = tool_available("tar");

        struct Case {
            name: &'static str,
            expect_warning: bool,
            needs: Option<(&'static str, bool)>,
        }
        let cases = vec![
            Case { name: "dirty.jpg", expect_warning: false, needs: None },
            Case { name: "dirty.png", expect_warning: false, needs: None },
            // 0.15.0 intentionally keeps structural fields in cleaned PDFs
            // (creation-date:-1, format:PDF-1.x, mod-date:-1 — "Don't change
            // the PDF version of cleaned files"); detectable => Warning, by design
            Case { name: "dirty.pdf", expect_warning: true, needs: None },
            Case { name: "dirty.docx", expect_warning: false, needs: None },
            Case { name: "dirty.mp3", expect_warning: false, needs: None },
            Case { name: "dirty.flac", expect_warning: false, needs: None },
            Case { name: "dirty.mp4", expect_warning: true, needs: Some(("ffmpeg", has_ffmpeg)) },
        ];

        let mut ran = 0;
        for case in cases {
            if let Some((tool, available)) = case.needs {
                if !available {
                    eprintln!("SKIP {}: optional dependency {} unavailable", case.name, tool);
                    continue;
                }
            }
            let dir = tempdir(&format!("fmt-{}", case.name.replace('.', "_")));
            let src = dir.join(case.name);
            fs::copy(fixture(case.name), &src).unwrap();
            let sha_before = sha256_file(&src).unwrap();

            let events = MockEvents::default();
            let report = run_job(&rt, vec![item("f", src.clone())], &JobSettings::default(), &events, &AtomicBool::new(false));
            let r = &report.results[0];

            let expected = if case.expect_warning { FileStatus::Warning } else { FileStatus::Processed };
            assert_eq!(r.status, expected, "{} => {:?}: {}", case.name, r.status, r.detail);
            assert_eq!(sha256_file(&src).unwrap(), sha_before, "{} source must be unchanged", case.name);

            let outputs = walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME));
            assert_eq!(outputs.len(), 1, "{} one committed output: {:?}", case.name, outputs);
            assert!(fs::metadata(&outputs[0]).unwrap().len() > 0, "{} output non-empty", case.name);
            let summary = r.summary.expect("summary present");
            if case.expect_warning {
                assert!(summary.still_detectable > 0, "{} should keep structural metadata", case.name);
            } else {
                assert_eq!(summary.still_detectable, 0, "{} post: {:?}", case.name, r.diffs);
            }
            ran += 1;
            fs::remove_dir_all(&dir).unwrap();
        }
        assert!(ran >= 6, "expected most formats to run, only {ran} did");

        if has_zip {
            let dir = tempdir("fmt-zip");
            fs::copy(fixture("dirty.jpg"), dir.join("member.jpg")).unwrap();
            fs::copy(fixture("dirty.png"), dir.join("member.png")).unwrap();
            let zip_path = dir.join("archive.zip");
            let status = std::process::Command::new("zip")
                .arg("-j").arg(&zip_path).arg(dir.join("member.jpg")).arg(dir.join("member.png"))
                .status().unwrap();
            assert!(status.success());
            fs::remove_file(dir.join("member.jpg")).unwrap();
            fs::remove_file(dir.join("member.png")).unwrap();

            let events = MockEvents::default();
            let report = run_job(&rt, vec![item("z", zip_path.clone())], &JobSettings::default(), &events, &AtomicBool::new(false));
            let r = &report.results[0];
            assert_eq!(r.status, FileStatus::Processed, "zip => {:?}: {}", r.status, r.detail);
            assert!(!walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME)).is_empty());
            fs::remove_dir_all(&dir).unwrap();
            ran += 1;
        } else {
            eprintln!("SKIP archive.zip: zip CLI unavailable");
        }

        if has_tar {
            let dir = tempdir("fmt-tar");
            fs::copy(fixture("dirty.jpg"), dir.join("member.jpg")).unwrap();
            let tar_path = dir.join("archive.tar");
            let status = std::process::Command::new("tar")
                .env("COPYFILE_DISABLE", "1")
                .arg("-cf").arg(&tar_path).arg("-C").arg(&dir).arg("member.jpg")
                .status().unwrap();
            assert!(status.success());
            fs::remove_file(dir.join("member.jpg")).unwrap();

            let events = MockEvents::default();
            let report = run_job(&rt, vec![item("t", tar_path.clone())], &JobSettings::default(), &events, &AtomicBool::new(false));
            let r = &report.results[0];
            assert_eq!(r.status, FileStatus::Processed, "tar => {:?}: {}", r.status, r.detail);
            fs::remove_dir_all(&dir).unwrap();
            ran += 1;
        } else {
            eprintln!("SKIP archive.tar: tar CLI unavailable");
        }
        eprintln!("format matrix: {ran} cases ran");
    }

    #[test]
    fn integration_unknown_member_policies() {
        let Some(rt) = runtime_or_skip() else { return };
        if !tool_available("zip") {
            eprintln!("SKIP unknown-member policies: zip CLI unavailable");
            return;
        }
        let dir = tempdir("policy-zip");
        fs::copy(fixture("dirty.jpg"), dir.join("photo.jpg")).unwrap();
        fs::write(dir.join("evil.py"), b"print('unsupported member')\n").unwrap();
        let zip_path = dir.join("mixed.zip");
        let status = std::process::Command::new("zip")
            .arg("-j").arg(&zip_path).arg(dir.join("photo.jpg")).arg(dir.join("evil.py"))
            .status().unwrap();
        assert!(status.success());

        // abort (default): unsupported member => clean fails, no output
        let events = MockEvents::default();
        let report = run_job(&rt, vec![item("a", zip_path.clone())], &JobSettings::default(), &events, &AtomicBool::new(false));
        assert_eq!(report.results[0].status, FileStatus::Failed, "{:?}", report.results[0]);
        assert!(walkdir_flat(&dir.join(crate::output::OUTPUT_DIR_NAME)).is_empty());

        // omit: unsupported member dropped => success
        let events = MockEvents::default();
        let settings = JobSettings { unknown_members: crate::mat2_runner::UnknownMembers::Omit, ..Default::default() };
        let report = run_job(&rt, vec![item("o", zip_path.clone())], &settings, &events, &AtomicBool::new(false));
        assert!(
            report.results[0].status == FileStatus::Processed || report.results[0].status == FileStatus::Warning,
            "omit => {:?}", report.results[0]
        );

        // keep: never a hard failure (may retain metadata => Warning allowed)
        let events = MockEvents::default();
        let settings = JobSettings { unknown_members: crate::mat2_runner::UnknownMembers::Keep, ..Default::default() };
        let report = run_job(&rt, vec![item("k", zip_path)], &settings, &events, &AtomicBool::new(false));
        assert_ne!(report.results[0].status, FileStatus::Failed, "keep => {:?}", report.results[0]);
        fs::remove_dir_all(&dir).unwrap();
    }

    fn walkdir_flat(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
        out
    }
}
