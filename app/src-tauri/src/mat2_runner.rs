use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::{InspectionResult, parse_inspection_stdout};

/// Dev-tree manifest root for debug binaries. Release binaries must not
/// embed build-machine paths (public artifacts carry no build-host
/// metadata); the dev fallback is meaningless there because bundled
/// resources always resolve first via the resource hint.
#[cfg(debug_assertions)]
pub(crate) fn dev_manifest_dir() -> Option<PathBuf> {
    Some(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf())
}

#[cfg(not(debug_assertions))]
pub(crate) fn dev_manifest_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    for _ in 0..6 {
        if dir.file_name().map(|n| n == "src-tauri").unwrap_or(false)
            && dir.join("resources/mat2_inspect.py").exists()
        {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum UnknownMembers {
    #[default]
    Abort,
    Omit,
    Keep,
}

impl UnknownMembers {
    pub fn as_str(self) -> &'static str {
        match self {
            UnknownMembers::Abort => "abort",
            UnknownMembers::Omit => "omit",
            UnknownMembers::Keep => "keep",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CleanOptions {
    pub lightweight: bool,
    pub verbose: bool,
    pub unknown_members: UnknownMembers,
    pub inplace: bool,
}

#[derive(Clone, Debug)]
pub struct Mat2Output {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RuntimeKind {
    /// Development: project venv Python + supplied upstream script.
    Dev,
    /// Packaged: self-contained PyInstaller runtime (Task 19).
    Frozen,
}

#[derive(Clone, Debug)]
pub struct Mat2Runtime {
    pub program: PathBuf,
    /// Args between program and MAT2 flags: dev = [script]; frozen = ["mat2"].
    pub cli_prefix: Vec<OsString>,
    /// Args before the file for structured inspection:
    /// dev = [adapter, upstream_dir]; frozen = ["inspect", "bundled"].
    pub inspect_prefix: Vec<OsString>,
    pub kind: RuntimeKind,
}

/// Mirrors upstream libmat2/abstract.py: output = fname + ".cleaned" + extension,
/// with the tar.gz/tar.bz2/… special case. MAT2 writes it beside the input file.
pub fn expected_cleaned_path(input: &Path) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input.file_stem().unwrap_or_default();
    let ext = input.extension();
    let stem_s = stem.to_string_lossy();

    if stem_s.ends_with(".tar") && stem_s.len() > 4 {
        let mut out = OsString::from(&stem_s[..stem_s.len() - 4]);
        out.push(".cleaned.tar");
        if let Some(e) = ext {
            out.push(".");
            out.push(e);
        }
        return parent.join(out);
    }

    let mut out = stem.to_os_string();
    out.push(".cleaned");
    if let Some(e) = ext {
        out.push(".");
        out.push(e);
    }
    parent.join(out)
}

#[cfg(test)]
fn inspect_argv(file: &Path, verbose: bool) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::new();
    if verbose {
        argv.push("--verbose".into());
    }
    argv.push("--show".into());
    // SECURITY INVARIANT: `--` ends option parsing so filenames beginning with
    // `-` (or any hostile name) can never be reinterpreted as flags.
    argv.push("--".into());
    argv.push(file.as_os_str().to_os_string());
    argv
}

fn clean_argv(file: &Path, opts: &CleanOptions) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::new();
    if opts.verbose {
        argv.push("--verbose".into());
    }
    if opts.lightweight {
        argv.push("--lightweight".into());
    }
    if opts.inplace {
        argv.push("--inplace".into());
    }
    argv.push("--unknown-members".into());
    argv.push(opts.unknown_members.as_str().into());
    argv.push("--".into());
    argv.push(file.as_os_str().to_os_string());
    argv
}

impl Mat2Runtime {
    /// Locate the supplied MAT2 runtime. Resolution order: env overrides
    /// (MAT2_WRAPPER_PYTHON + MAT2_WRAPPER_SCRIPT for the dev shape, or
    /// MAT2_WRAPPER_RUNTIME_BIN for the frozen shape), then the packaged
    /// runtime inside the app resource dir (frozen, Task 19), then the
    /// development tree (project-root/.venv/bin/python + upstream-mat2/mat2,
    /// derived from the compile-time manifest dir), then Err — callers must
    /// fail visibly, never fall back to a global `mat2` or any alternate
    /// engine (HANDOFF §3).
    #[cfg(test)]
    pub fn resolve() -> Result<Mat2Runtime, String> {
        Self::resolve_with_hint(None)
    }

    pub fn resolve_with_hint(resource_dir: Option<&Path>) -> Result<Mat2Runtime, String> {
        if let (Ok(python), Ok(script)) = (
            std::env::var("MAT2_WRAPPER_PYTHON"),
            std::env::var("MAT2_WRAPPER_SCRIPT"),
        ) {
            let python = PathBuf::from(python);
            let script = PathBuf::from(script);
            if python.exists() && script.exists() {
                return Ok(Self::dev(python, script));
            }
            return Err(format!(
                "MAT2_WRAPPER_PYTHON/MAT2_WRAPPER_SCRIPT point at missing files: {:?} / {:?}",
                python, script
            ));
        }
        if let Ok(bin) = std::env::var("MAT2_WRAPPER_RUNTIME_BIN") {
            let bin = PathBuf::from(bin);
            if bin.exists() {
                return Ok(Self::frozen(bin));
            }
            return Err(format!(
                "MAT2_WRAPPER_RUNTIME_BIN points at a missing file: {:?}",
                bin
            ));
        }
        if let Some(res) = resource_dir {
            let bin = res.join("mat2-runtime").join("mat2-runtime");
            if bin.exists() {
                return Ok(Self::frozen(bin));
            }
        }

        if let Some(manifest) = dev_manifest_dir()
            && let Some(root) = manifest.ancestors().nth(2)
        {
            let python = root.join(".venv/bin/python");
            let script = root.join("upstream-mat2/mat2");
            if python.exists() && script.exists() {
                return Ok(Self::dev(python, script));
            }
        }

        Err("MAT2 runtime not found (looked for: env overrides, bundled resources/mat2-runtime, <project>/.venv/bin/python + <project>/upstream-mat2/mat2)".to_string())
    }

    fn dev(python: PathBuf, script: PathBuf) -> Mat2Runtime {
        let upstream_dir = script
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let adapter = Self::adapter_path().unwrap_or_else(|_| upstream_dir.join("mat2_inspect.py"));
        Mat2Runtime {
            program: python,
            cli_prefix: vec![script.into()],
            inspect_prefix: vec![adapter.into(), upstream_dir.into()],
            kind: RuntimeKind::Dev,
        }
    }

    fn frozen(bin: PathBuf) -> Mat2Runtime {
        Mat2Runtime {
            program: bin,
            cli_prefix: vec![OsString::from("mat2")],
            inspect_prefix: vec![OsString::from("inspect"), OsString::from("bundled")],
            kind: RuntimeKind::Frozen,
        }
    }

    fn base_command(&self) -> Command {
        // SECURITY INVARIANT: the program is always the pinned runtime
        // (venv Python + supplied MAT2 script, or the verified frozen bundle)
        // invoked with an argv vector. No shell is ever involved; no user
        // data reaches a command string.
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.cli_prefix);
        cmd
    }

    fn run(&self, argv: Vec<OsString>) -> Result<Mat2Output, String> {
        let out = self
            .base_command()
            .args(&argv)
            .output()
            .map_err(|e| format!("failed to spawn MAT2 runtime: {e}"))?;
        Ok(Mat2Output {
            exit_code: out.status.code(),
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    pub fn version(&self) -> Result<Mat2Output, String> {
        self.run(vec!["--version".into()])
    }

    pub fn help(&self) -> Result<Mat2Output, String> {
        self.run(vec!["--help".into()])
    }

    pub fn list_formats(&self) -> Result<Mat2Output, String> {
        self.run(vec!["--list".into()])
    }

    pub fn check_dependencies(&self) -> Result<Mat2Output, String> {
        self.run(vec!["--check-dependencies".into()])
    }

    /// `--show` mode. WARNING: upstream always exits 0 in show mode, even for
    /// unsupported or missing files — callers must parse stdout, never trust
    /// `success` alone here (docs/UPSTREAM_SNAPSHOT.md §4.5).
    #[cfg(test)]
    pub fn inspect(&self, file: &Path, verbose: bool) -> Result<Mat2Output, String> {
        self.run(inspect_argv(file, verbose))
    }

    /// Structured inspection via the read-only libmat2 JSON adapter
    /// (resources/mat2_inspect.py in dev; the frozen bundle's `inspect`
    /// subcommand when packaged). Same libmat2 API the CLI uses; no
    /// sanitisation semantics involved. Same no-shell invariant: fixed
    /// program + argv vector.
    pub fn inspect_json(&self, file: &Path) -> Result<InspectionResult, String> {
        let out = Command::new(&self.program)
            .args(&self.inspect_prefix)
            .arg(file)
            .output()
            .map_err(|e| format!("failed to spawn inspection adapter: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "inspection adapter failed (exit {:?}): {}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        parse_inspection_stdout(&String::from_utf8_lossy(&out.stdout))
    }

    pub fn adapter_path() -> Result<PathBuf, String> {
        if let Ok(p) = std::env::var("MAT2_WRAPPER_INSPECT_ADAPTER") {
            let path = PathBuf::from(p);
            if path.exists() {
                return Ok(path);
            }
            return Err(format!(
                "MAT2_WRAPPER_INSPECT_ADAPTER points at a missing file: {:?}",
                path
            ));
        }
        if let Some(manifest) = dev_manifest_dir() {
            let dev = manifest.join("resources/mat2_inspect.py");
            if dev.exists() {
                return Ok(dev);
            }
        }
        Err("inspection adapter mat2_inspect.py not found".to_string())
    }

    /// Normal (or lightweight/inplace) cleaning of a single file.
    /// Exit 0 = success; 255 (-1) = upstream signalled failure.
    #[cfg(test)]
    pub fn clean(&self, file: &Path, opts: &CleanOptions) -> Result<Mat2Output, String> {
        match self.clean_cancellable(file, opts, &std::sync::atomic::AtomicBool::new(false))? {
            CleanRunOutcome::Completed(out) => Ok(out),
            CleanRunOutcome::Cancelled => Err("clean cancelled".to_string()),
        }
    }

    /// Cleaning with cooperative cancellation. The child runs in its own
    /// process group; on cancel the whole group gets SIGTERM, a grace period,
    /// then SIGKILL — MAT2's internal ProcessPoolExecutor workers cannot be
    /// orphaned. stdout/stderr are captured via temp files (no pipe deadlock
    /// while polling).
    pub fn clean_cancellable(
        &self,
        file: &Path,
        opts: &CleanOptions,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<CleanRunOutcome, String> {
        use std::process::Stdio;

        let out_path = std::env::temp_dir().join(format!("mat2-out-{}", uuid::Uuid::new_v4()));
        let err_path = std::env::temp_dir().join(format!("mat2-err-{}", uuid::Uuid::new_v4()));
        let out_file = std::fs::File::create(&out_path)
            .map_err(|e| format!("cannot create stdout capture: {e}"))?;
        let err_file = std::fs::File::create(&err_path)
            .map_err(|e| format!("cannot create stderr capture: {e}"))?;

        let mut cmd = self.base_command();
        cmd.args(clean_argv(file, opts))
            .stdin(Stdio::null())
            .stdout(Stdio::from(out_file))
            .stderr(Stdio::from(err_file));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn MAT2 runtime: {e}"))?;

        let status = loop {
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                terminate_group(&mut child);
                let _ = std::fs::remove_file(&out_path);
                let _ = std::fs::remove_file(&err_path);
                return Ok(CleanRunOutcome::Cancelled);
            }
            let wait = child.try_wait();
            match wait {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(40)),
                Err(e) => {
                    terminate_group(&mut child);
                    let _ = std::fs::remove_file(&out_path);
                    let _ = std::fs::remove_file(&err_path);
                    return Err(format!("wait failed: {e}"));
                }
            }
        };

        let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
        let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
        let _ = std::fs::remove_file(&out_path);
        let _ = std::fs::remove_file(&err_path);

        Ok(CleanRunOutcome::Completed(Mat2Output {
            exit_code: status.code(),
            success: status.success(),
            stdout,
            stderr,
        }))
    }
}

pub enum CleanRunOutcome {
    Completed(Mat2Output),
    Cancelled,
}

use crate::model::{DependencyStatus, DiagnosticsDto};

/// Parse `mat2 --check-dependencies` stdout:
/// "- Cairo: yes " / "- Exiftool: no (optional)" lines.
pub fn parse_dependency_report(stdout: &str) -> Vec<DependencyStatus> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("- ") else {
            continue;
        };
        let Some((name, status)) = rest.split_once(':') else {
            continue;
        };
        let status = status.trim();
        if status.is_empty() {
            continue;
        }
        out.push(DependencyStatus {
            name: name.trim().to_string(),
            found: status.starts_with("yes"),
            required: !status.contains("(optional)"),
        });
    }
    out
}

pub fn diagnostics_unavailable(reason: &str, app_version: &str) -> DiagnosticsDto {
    DiagnosticsDto {
        available: false,
        fatal: true,
        version: None,
        dependencies: Vec::new(),
        missing_required: Vec::new(),
        missing_optional: Vec::new(),
        error: Some(reason.to_string()),
        app_version: app_version.to_string(),
    }
}

/// Pure mapping from raw runtime query results to the startup DTO:
/// fatal whenever the runtime cannot be queried or a REQUIRED dependency is
/// missing (processing must be disabled, never best-effort — HANDOFF §9.4,
/// plan Task 15). Missing OPTIONAL deps degrade formats visibly, not fatally.
pub fn build_diagnostics(
    version: Result<Mat2Output, String>,
    deps: Result<Mat2Output, String>,
    app_version: &str,
) -> DiagnosticsDto {
    let version_str = match version {
        Err(e) => {
            return DiagnosticsDto {
                error: Some(e),
                ..diagnostics_unavailable("version query failed", app_version)
            };
        }
        Ok(v) if !v.success => {
            return DiagnosticsDto {
                error: Some(format!("version query exit {:?}", v.exit_code)),
                ..diagnostics_unavailable("version query failed", app_version)
            };
        }
        Ok(v) => v.stdout.trim().to_string(),
    };

    let dependencies = match deps {
        Err(e) => {
            return DiagnosticsDto {
                error: Some(e),
                version: Some(version_str),
                ..diagnostics_unavailable("dependency check failed", app_version)
            };
        }
        Ok(d) => parse_dependency_report(&d.stdout),
    };

    let missing_required: Vec<String> = dependencies
        .iter()
        .filter(|d| d.required && !d.found)
        .map(|d| d.name.clone())
        .collect();
    let missing_optional: Vec<String> = dependencies
        .iter()
        .filter(|d| !d.required && !d.found)
        .map(|d| d.name.clone())
        .collect();
    let fatal = !missing_required.is_empty();

    DiagnosticsDto {
        available: true,
        fatal,
        version: Some(version_str),
        dependencies,
        missing_required,
        missing_optional,
        error: None,
        app_version: app_version.to_string(),
    }
}

#[cfg(unix)]
fn terminate_group(child: &mut std::process::Child) {
    let pid = child.id() as libc::pid_t;
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    for _ in 0..10 {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_group(child: &mut std::process::Child) {
    let _ = child.start_kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_argv_normal() {
        let argv = clean_argv(Path::new("/tmp/x/photo.jpg"), &CleanOptions::default());
        let strs: Vec<String> = argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            strs,
            vec!["--unknown-members", "abort", "--", "/tmp/x/photo.jpg"]
        );
    }

    #[test]
    fn clean_argv_lightweight_verbose_policy_inplace() {
        let opts = CleanOptions {
            lightweight: true,
            verbose: true,
            unknown_members: UnknownMembers::Omit,
            inplace: true,
        };
        let argv = clean_argv(Path::new("/a/b.png"), &opts);
        let strs: Vec<String> = argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            strs,
            vec![
                "--verbose",
                "--lightweight",
                "--inplace",
                "--unknown-members",
                "omit",
                "--",
                "/a/b.png"
            ]
        );
    }

    #[test]
    fn inspect_argv_show() {
        let argv = inspect_argv(Path::new("/a/b.pdf"), false);
        let strs: Vec<String> = argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(strs, vec!["--show", "--", "/a/b.pdf"]);
        let argv_v = inspect_argv(Path::new("/a/b.pdf"), true);
        assert_eq!(argv_v[0], OsString::from("--verbose"));
    }

    #[test]
    fn hostile_filenames_are_single_data_arguments() {
        let hostile = [
            "--version.jpg",
            "--help.png",
            "-L.pdf",
            "; touch owned\".jpg",
            "$(touch owned).jpg",
            "`id`.png",
            "a'b\"c.jpg",
            "<img src=x onerror=alert(1)>.jpg",
            "pipe|and&amp.jpeg",
            "emoji-🎉-file.jpg",
            "new\nline.jpg",
            "tab\tand\u{1b}[31mansi.jpg",
            " leading-space.jpg",
        ];
        for name in hostile {
            let p = Path::new("/tmp/batch").join(name);
            let argv = clean_argv(&p, &CleanOptions::default());
            // last argv element is exactly the path — one argument, unmodified
            assert_eq!(argv.last().unwrap(), p.as_os_str());
            // the path always comes after the end-of-options marker
            let dd = argv.iter().position(|a| a == "--").expect("no -- marker");
            assert_eq!(
                dd,
                argv.len() - 2,
                "file not immediately after -- for {name:?}"
            );
            // no argv element besides the path contains the hostile payload
            for a in &argv[..dd] {
                assert!(
                    !a.to_string_lossy().contains(name),
                    "flag slot polluted by {name:?}"
                );
            }
        }
    }

    #[test]
    fn invocation_is_never_a_shell() {
        let rt = Mat2Runtime {
            program: PathBuf::from("/usr/bin/true"),
            cli_prefix: vec![OsString::from("/script/mat2")],
            inspect_prefix: vec![OsString::from("inspect"), OsString::from("bundled")],
            kind: RuntimeKind::Dev,
        };
        let mut cmd = rt.base_command();
        cmd.args(clean_argv(
            Path::new("/tmp/a.jpg"),
            &CleanOptions::default(),
        ));
        let prog = cmd.get_program().to_string_lossy().into_owned();
        assert_eq!(prog, "/usr/bin/true");
        for shell in ["sh", "bash", "zsh", "/bin/sh", "/bin/bash", "/bin/zsh"] {
            assert!(!prog.ends_with(shell), "program must never be a shell");
        }
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "/script/mat2");
        // never a `-c` style concatenated command string
        assert!(!args.iter().any(|a| a == "-c"));
    }

    #[test]
    fn expected_cleaned_paths_mirror_upstream() {
        assert_eq!(
            expected_cleaned_path(Path::new("/d/photo.jpg")),
            PathBuf::from("/d/photo.cleaned.jpg")
        );
        assert_eq!(
            expected_cleaned_path(Path::new("/d/archive.tar.gz")),
            PathBuf::from("/d/archive.cleaned.tar.gz")
        );
        assert_eq!(
            expected_cleaned_path(Path::new("/d/plain")),
            PathBuf::from("/d/plain.cleaned")
        );
        assert_eq!(
            expected_cleaned_path(Path::new("/d/.bashrc")),
            PathBuf::from("/d/.bashrc.cleaned")
        );
        assert_eq!(
            expected_cleaned_path(Path::new("/d/report.DOCX")),
            PathBuf::from("/d/report.cleaned.DOCX")
        );
    }

    fn runtime_or_skip() -> Option<Mat2Runtime> {
        match Mat2Runtime::resolve() {
            Ok(rt) => Some(rt),
            Err(e) => {
                eprintln!("SKIP integration: {e}");
                None
            }
        }
    }

    fn tempdir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p =
            std::env::temp_dir().join(format!("mat2run-{}-{}-{}", tag, std::process::id(), nanos));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn dependency_report_parsing() {
        let stdout = "Dependencies for mat2 0.15.0:\n- Cairo: yes \n- Exiftool: no (optional)\n- Mutagen: yes \n- Ffmpeg: no (optional)\n";
        let deps = parse_dependency_report(stdout);
        assert_eq!(deps.len(), 4);
        assert_eq!(
            deps[0],
            DependencyStatus {
                name: "Cairo".into(),
                found: true,
                required: true
            }
        );
        assert_eq!(
            deps[1],
            DependencyStatus {
                name: "Exiftool".into(),
                found: false,
                required: false
            }
        );
        assert!(parse_dependency_report("nonsense\nno dash here").is_empty());
    }

    #[test]
    fn diagnostics_healthy_and_fatal_cases() {
        let ok_ver = Mat2Output {
            exit_code: Some(0),
            success: true,
            stdout: "mat2 0.15.0\n".into(),
            stderr: String::new(),
        };
        let ok_deps = Mat2Output {
            exit_code: Some(0),
            success: true,
            stdout: "- Cairo: yes\n- Exiftool: no (optional)\n".into(),
            stderr: String::new(),
        };

        let d = build_diagnostics(Ok(ok_ver.clone()), Ok(ok_deps.clone()), "0.1.0");
        assert!(d.available && !d.fatal, "{:?}", d);
        assert_eq!(d.version.as_deref(), Some("mat2 0.15.0"));
        assert_eq!(d.missing_optional, vec!["Exiftool".to_string()]);
        assert_eq!(d.app_version, "0.1.0");

        let bad_deps = Mat2Output {
            stdout: "- Poppler from PyGobject: no\n- Cairo: yes\n".into(),
            ..ok_deps.clone()
        };
        let d2 = build_diagnostics(Ok(ok_ver.clone()), Ok(bad_deps), "0.1.0");
        assert!(d2.fatal && d2.available);
        assert_eq!(
            d2.missing_required,
            vec!["Poppler from PyGobject".to_string()]
        );

        let d3 = build_diagnostics(Err("spawn failed".into()), Ok(ok_deps.clone()), "0.1.0");
        assert!(!d3.available && d3.fatal && d3.error.is_some());

        let failed_ver = Mat2Output {
            success: false,
            exit_code: Some(2),
            ..ok_ver.clone()
        };
        let d4 = build_diagnostics(Ok(failed_ver), Ok(ok_deps), "0.1.0");
        assert!(d4.fatal && !d4.available);
    }

    #[test]
    fn integration_diagnostics_healthy_runtime() {
        let Some(rt) = runtime_or_skip() else { return };
        let d = build_diagnostics(rt.version(), rt.check_dependencies(), "test");
        assert!(d.available && !d.fatal, "{:?}", d);
        assert!(d.version.unwrap().contains("0.15.0"));
        assert!(d.missing_required.is_empty());
    }

    #[test]
    fn integration_diagnostics_and_clean_roundtrip() {
        let Some(rt) = runtime_or_skip() else { return };

        let v = rt.version().unwrap();
        assert!(v.success && v.stdout.contains("mat2 "), "version: {:?}", v);

        let l = rt.list_formats().unwrap();
        assert!(
            l.success && l.stdout.contains("image/jpeg"),
            "list: {:?}",
            l
        );

        let d = rt.check_dependencies().unwrap();
        assert!(d.success && d.stdout.contains("Poppler"), "deps: {:?}", d);

        let h = rt.help().unwrap();
        assert!(
            h.success && h.stdout.contains("--unknown-members"),
            "help: {:?}",
            h
        );

        let dir = tempdir("roundtrip");
        let fixture = dir.join("dirty.jpg");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.ancestors().nth(2).unwrap();
        std::fs::copy(root.join("upstream-mat2/tests/data/dirty.jpg"), &fixture).unwrap();

        let shown = rt.inspect(&fixture, false).unwrap();
        assert!(shown.success);
        assert!(
            shown.stdout.contains("Comment: Created with GIMP"),
            "inspect: {:?}",
            shown.stdout
        );

        let cleaned = rt.clean(&fixture, &CleanOptions::default()).unwrap();
        assert!(cleaned.success, "clean failed: {:?}", cleaned);
        let out_path = expected_cleaned_path(&fixture);
        assert!(out_path.exists(), "expected output missing: {:?}", out_path);
        assert!(std::fs::metadata(&out_path).unwrap().len() > 0);

        let after = rt.inspect(&out_path, false).unwrap();
        assert!(
            after.stdout.contains("No metadata found"),
            "post-inspect: {:?}",
            after.stdout
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn integration_dash_filename_is_cleaned_as_data() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("dash");
        let fixture = dir.join("-dash-name.png");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.ancestors().nth(2).unwrap();
        std::fs::copy(root.join("upstream-mat2/tests/data/dirty.png"), &fixture).unwrap();

        let res = rt.clean(&fixture, &CleanOptions::default()).unwrap();
        assert!(res.success, "clean of dash-file failed: {:?}", res);
        let out = expected_cleaned_path(&fixture);
        assert!(out.exists(), "missing {:?}", out);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn integration_show_mode_exit_code_is_not_trustworthy() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("unsupported");
        let weird = dir.join("notes.unknownext123");
        std::fs::write(&weird, b"hello").unwrap();

        let shown = rt.inspect(&weird, false).unwrap();
        // documents the false-success trap: exit 0 despite "not supported"
        assert!(shown.success, "show mode should exit 0: {:?}", shown);
        assert!(
            shown.stdout.contains("not supported"),
            "stdout: {:?}",
            shown.stdout
        );

        let structured = rt.inspect_json(&weird).unwrap();
        assert!(structured.error.is_none());
        assert!(!structured.supported);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn integration_adapter_matches_cli_show() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("adaptercli");
        let fixture = dir.join("dirty.jpg");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.ancestors().nth(2).unwrap();
        std::fs::copy(root.join("upstream-mat2/tests/data/dirty.jpg"), &fixture).unwrap();

        let cli = rt.inspect(&fixture, false).unwrap();
        let structured = rt.inspect_json(&fixture).unwrap();
        assert!(cli.stdout.contains("Comment: Created with GIMP"));
        assert!(structured.supported && structured.error.is_none());
        assert_eq!(structured.mimetype.as_deref(), Some("image/jpeg"));
        assert!(
            structured
                .entries
                .iter()
                .any(|e| e.key == "Comment" && e.display_value == "Created with GIMP")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn integration_adapter_flattens_nested_archive_metadata() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("adapternested");
        let fixture = dir.join("dirty.docx");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.ancestors().nth(2).unwrap();
        std::fs::copy(root.join("upstream-mat2/tests/data/dirty.docx"), &fixture).unwrap();

        let structured = rt.inspect_json(&fixture).unwrap();
        assert!(structured.supported, "{:?}", structured);
        assert!(!structured.entries.is_empty());
        assert!(
            structured.entries.iter().any(|e| e.key.contains(" / ")),
            "expected member-nested keys, got {:?}",
            structured
                .entries
                .iter()
                .map(|e| &e.key)
                .collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn integration_adapter_reports_missing_file_as_error() {
        let Some(rt) = runtime_or_skip() else { return };
        let dir = tempdir("adaptermissing");
        let missing = dir.join("ghost.jpg");
        let structured = rt.inspect_json(&missing).unwrap();
        assert!(structured.error.is_some() || !structured.supported);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
