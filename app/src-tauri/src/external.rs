use std::path::PathBuf;
use std::process::Command;

const MAT2_URL: &str = "https://github.com/jvoisin/mat2";
const DANGERZONE_URL: &str = "https://github.com/freedomofpress/dangerzone";
const PRIVACYTOOLS_URL: &str = "https://www.privacytools.io/";

/// SECURITY INVARIANT: the only three external destinations this application
/// can ever open. They are compile-time constants; no command in this crate
/// accepts a URL or a path from the frontend. Opened in the system browser
/// via the platform opener binary with an argv vector — never a shell, never
/// user input, no query parameters, no tracking data appended.
const APPROVED_URLS: [&str; 3] = [MAT2_URL, DANGERZONE_URL, PRIVACYTOOLS_URL];

fn open_approved_url(url: &str) -> Result<(), String> {
    if !APPROVED_URLS.contains(&url) {
        return Err("URL is not on the hard-coded approved list".into());
    }
    if url.contains('?') || url.contains('#') {
        return Err("approved URLs must carry no query or fragment".into());
    }
    platform_open(&[url])
}

pub fn open_mat2_site() -> Result<(), String> {
    open_approved_url(MAT2_URL)
}

pub fn open_dangerzone_site() -> Result<(), String> {
    open_approved_url(DANGERZONE_URL)
}

pub fn open_privacytools_site() -> Result<(), String> {
    open_approved_url(PRIVACYTOOLS_URL)
}

/// Compute the unique parent directories of committed outputs (order kept).
/// Pure logic, unit-tested; reveal_output in lib.rs feeds it only paths that
/// the job pipeline itself recorded.
pub fn reveal_dirs(output_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for p in output_paths {
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !dirs.iter().any(|d| d == parent) {
                dirs.push(parent.to_path_buf());
            }
        }
    }
    dirs
}

pub fn reveal_paths(dirs: &[PathBuf]) -> Result<(), String> {
    if dirs.is_empty() {
        return Err("job has no committed outputs to reveal".into());
    }
    for d in dirs {
        if !d.is_dir() {
            return Err(format!("output directory no longer exists: {:?}", d.file_name()));
        }
    }
    let args: Vec<&str> = dirs.iter().filter_map(|d| d.to_str()).collect();
    if args.len() != dirs.len() {
        return Err("output directory path is not representable".into());
    }
    platform_open(&args)
}

#[cfg(target_os = "macos")]
fn platform_open(args: &[&str]) -> Result<(), String> {
    Command::new("open")
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot launch system opener: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn platform_open(_args: &[&str]) -> Result<(), String> {
    Err("external opening is only implemented for the macOS v1 target".into())
}

#[allow(dead_code)]
pub fn approved_urls() -> &'static [&'static str] {
    &APPROVED_URLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_three_hardcoded_destinations() {
        let source = include_str!("external.rs");
        let const_decls = source.matches("&str = \"https://").count();
        assert_eq!(
            const_decls, 3,
            "external.rs must declare exactly 3 URL constants"
        );
        assert_eq!(APPROVED_URLS.len(), 3);
        assert_eq!(MAT2_URL, "https://github.com/jvoisin/mat2");
        assert_eq!(DANGERZONE_URL, "https://github.com/freedomofpress/dangerzone");
        assert_eq!(PRIVACYTOOLS_URL, "https://www.privacytools.io/");
    }

    #[test]
    fn no_query_or_tracking_parameters() {
        for url in APPROVED_URLS {
            assert!(!url.contains('?'), "query params forbidden: {url}");
            assert!(!url.contains("utm"), "UTM forbidden: {url}");
            assert!(!url.contains('#'));
        }
    }

    #[test]
    fn unapproved_urls_are_rejected_without_spawning() {
        for bad in [
            "https://evil.example.com",
            "https://github.com/jvoisin/mat2?utm=x",
            "http://github.com/jvoisin/mat2",
            "file:///etc/passwd",
            "",
        ] {
            assert!(open_approved_url(bad).is_err(), "must reject {bad:?}");
        }
    }

    #[test]
    fn reveal_dirs_unique_parents_in_order() {
        let paths = vec![
            PathBuf::from("/a/b/out1.cleaned.jpg"),
            PathBuf::from("/a/b/out2.cleaned.png"),
            PathBuf::from("/c/d/sub/out3.cleaned.pdf"),
        ];
        assert_eq!(
            reveal_dirs(&paths),
            vec![PathBuf::from("/a/b"), PathBuf::from("/c/d/sub")]
        );
        assert!(reveal_dirs(&[]).is_empty());
    }

    #[test]
    fn reveal_rejects_empty_and_missing_dirs() {
        assert!(reveal_paths(&[]).is_err());
        let missing = PathBuf::from("/definitely/not/here-12345");
        assert!(reveal_paths(&[missing]).is_err());
    }
}
