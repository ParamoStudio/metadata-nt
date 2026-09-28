use std::path::PathBuf;
use std::process::Command;

const MAT2_URL: &str = "https://github.com/jvoisin/mat2";
const DANGERZONE_URL: &str = "https://github.com/freedomofpress/dangerzone";
const PRIVACYTOOLS_URL: &str = "https://www.privacytools.io/";
const CANARYTOKENS_URL: &str = "https://canarytokens.org/";
const CANARY_DOCS_URL: &str = "https://docs.canarytokens.org/guide/fast-redirect-token.html";
const CANARY_REPO_URL: &str = "https://github.com/thinkst/canarytokens";
const CANARY_AUDIT_URL: &str = "https://resources.canary.tools/documents/Doyensec_ThinkstCanaryTokensOSS_Report_Q22024_WithRetesting.pdf";

/// SECURITY INVARIANT: the only external destinations this application
/// can ever open. They are compile-time constants; no command in this crate
/// accepts a URL or a path from the frontend. Opened in the system browser
/// via the platform opener binary with an argv vector — never a shell, never
/// user input, no query parameters, no tracking data appended.
/// The four Canarytokens destinations serve the Investigation Tripwire info
/// dialog (owner-approved add-on spec §21).
const APPROVED_URLS: [&str; 7] = [
    MAT2_URL,
    DANGERZONE_URL,
    PRIVACYTOOLS_URL,
    CANARYTOKENS_URL,
    CANARY_DOCS_URL,
    CANARY_REPO_URL,
    CANARY_AUDIT_URL,
];

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

pub fn open_canarytokens_site() -> Result<(), String> {
    open_approved_url(CANARYTOKENS_URL)
}

pub fn open_canary_docs() -> Result<(), String> {
    open_approved_url(CANARY_DOCS_URL)
}

pub fn open_canary_repo() -> Result<(), String> {
    open_approved_url(CANARY_REPO_URL)
}

pub fn open_canary_audit() -> Result<(), String> {
    open_approved_url(CANARY_AUDIT_URL)
}

/// Opens the official metadata'nt release page. The URL is constructed in
/// updates.rs from a strictly validated release tag (git-ref charset only,
/// bounded length); this function re-checks the no-query/no-fragment rule
/// before handing it to the platform opener. It is the ONLY dynamic-URL open
/// path in the application and it can only ever reach
/// github.com/ParamoStudio/metadata-nt/releases/tag/<validated-tag>.
pub(crate) fn open_release_page(url: &str) -> Result<(), String> {
    if !url.starts_with("https://github.com/ParamoStudio/metadata-nt/releases/tag/") {
        return Err("release page URL is not the official repository release page".into());
    }
    if url.contains('?') || url.contains('#') {
        return Err("release URLs must carry no query or fragment".into());
    }
    platform_open(&[url])
}

/// Compute the unique parent directories of committed outputs (order kept).
/// Pure logic, unit-tested; reveal_output in lib.rs feeds it only paths that
/// the job pipeline itself recorded.
pub fn reveal_dirs(output_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for p in output_paths {
        if let Some(parent) = p.parent()
            && !parent.as_os_str().is_empty()
            && !dirs.iter().any(|d| d == parent)
        {
            dirs.push(parent.to_path_buf());
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
            return Err(format!(
                "output directory no longer exists: {:?}",
                d.file_name()
            ));
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
    fn exactly_seven_hardcoded_destinations() {
        let source = include_str!("external.rs");
        let const_decls = source.matches("&str = \"https://").count();
        assert_eq!(
            const_decls, 7,
            "external.rs must declare exactly 7 URL constants"
        );
        assert_eq!(APPROVED_URLS.len(), 7);
        assert_eq!(MAT2_URL, "https://github.com/jvoisin/mat2");
        assert_eq!(
            DANGERZONE_URL,
            "https://github.com/freedomofpress/dangerzone"
        );
        assert_eq!(PRIVACYTOOLS_URL, "https://www.privacytools.io/");
        assert_eq!(CANARYTOKENS_URL, "https://canarytokens.org/");
        assert_eq!(
            CANARY_DOCS_URL,
            "https://docs.canarytokens.org/guide/fast-redirect-token.html"
        );
        assert_eq!(CANARY_REPO_URL, "https://github.com/thinkst/canarytokens");
        assert_eq!(
            CANARY_AUDIT_URL,
            "https://resources.canary.tools/documents/Doyensec_ThinkstCanaryTokensOSS_Report_Q22024_WithRetesting.pdf"
        );
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
