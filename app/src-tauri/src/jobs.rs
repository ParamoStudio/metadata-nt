use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::mat2_runner::{expected_cleaned_path, CleanOptions, Mat2Runtime};
use crate::model::MetadataEntry;
use crate::output;

#[derive(Debug)]
pub enum PipelineError {
    Unsupported(String),
    Mat2Failure(String),
    OutputMissing(String),
    OutputInvalid(String),
    Commit(String),
    Io(String),
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
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// Normal-mode pipeline for ONE file (INTERFACE.md §14):
/// pre-inspect → stage byte-identical copy → MAT2 clean (never --inplace) →
/// validate output exists/regular/non-empty → post-inspect → plan collision-free
/// final path → exclusive commit. Any failure leaves no final output.
#[allow(clippy::too_many_arguments)]
pub fn clean_one(
    rt: &Mat2Runtime,
    ws: &JobWorkspace,
    source: &Path,
    opts: &CleanOptions,
    canonical_output_root: &Path,
    relative_dir: Option<&Path>,
) -> Result<CleanOutcome, PipelineError> {
    if opts.inplace {
        return Err(PipelineError::Io(
            "normal pipeline refuses --inplace; destructive mode has its own path".into(),
        ));
    }

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

    let stage = ws.stage_dir().map_err(PipelineError::Io)?;
    let file_name = source
        .file_name()
        .ok_or_else(|| PipelineError::Io("source has no file name".into()))?;
    let staged = stage.join(file_name);
    fs::copy(source, &staged).map_err(|e| PipelineError::Io(format!("staging copy failed: {e}")))?;

    let cleaned = rt
        .clean(&staged, opts)
        .map_err(PipelineError::Io)?;
    if !cleaned.success {
        let detail = first_line(&cleaned.stdout);
        return Err(PipelineError::Mat2Failure(if detail.is_empty() {
            format!("exit code {:?}", cleaned.exit_code)
        } else {
            detail
        }));
    }

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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mat2_runner::Mat2Runtime;
    use crate::output::{job_root, ensure_root, OutputMode};

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

        // second file: staged, then "cancel" before clean/commit
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
