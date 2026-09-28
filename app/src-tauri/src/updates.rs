//! GitHub release checker for metadata'nt (owner-approved addition).
//!
//! SECURITY INVARIANTS (spec: GITHUB_UPDATE_CHECKER prompt §8-§16):
//! - the ONLY network destination here is the hard-coded GitHub Releases API
//!   for ParamoStudio/metadata-nt; no URL or repository ever arrives from the
//!   frontend;
//! - requests carry no cookies, no auth token, no machine/installation
//!   identifier, no hostname, no username, no file or document data — only a
//!   static User-Agent and (optionally) a stored response ETag;
//! - HTTPS with normal TLS validation (ureq/rustls), short timeout, bounded
//!   response body, no redirects followed into other origins;
//! - nothing is ever downloaded or installed: an available update only
//!   produces a dialog offering to open the official release page;
//! - the release page URL is constructed locally from a strictly validated
//!   tag and opened through external.rs's platform opener;
//! - automatic checks happen only at launch, only with explicit first-run
//!   consent, only when the user-selected interval elapsed; no polling timer.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const RELEASES_ENDPOINT: &str =
    "https://api.github.com/repos/ParamoStudio/metadata-nt/releases/latest";
const RELEASE_PAGE_BASE: &str = "https://github.com/ParamoStudio/metadata-nt/releases/tag/";
const USER_AGENT: &str = "metadata-nt-update-checker";
const ACCEPT: &str = "application/vnd.github+json";
const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_BODY_BYTES: u64 = 256 * 1024;
const SETTINGS_FILE: &str = "update-settings.json";
const SECS_PER_DAY: u64 = 86_400;

pub const ALLOWED_INTERVALS_DAYS: [u64; 3] = [1, 7, 30];
pub const RECOMMENDED_INTERVAL_DAYS: u64 = 7;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    pub onboarding_completed: bool,
    pub automatic_update_checks_enabled: bool,
    pub update_check_interval_days: u64,
    pub last_update_check_at: Option<u64>,
    pub optional_github_etag: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            onboarding_completed: false,
            automatic_update_checks_enabled: false,
            update_check_interval_days: RECOMMENDED_INTERVAL_DAYS,
            last_update_check_at: None,
            optional_github_etag: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemVer {
    pub fn from_parts(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn display(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parses `v1.2.3` / `1.2.3` only. Prerelease/build suffixes, extra segments
/// and non-numeric parts are rejected so they can never be offered as updates.
pub fn parse_release_tag(tag: &str) -> Option<SemVer> {
    let trimmed = tag.strip_prefix('v').unwrap_or(tag);
    if trimmed.is_empty() || trimmed.len() > 32 {
        return None;
    }
    let mut parts = trimmed.split('.');
    let (maj, min, pat) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    for p in [maj, min, pat] {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    Some(SemVer::from_parts(
        maj.parse().ok()?,
        min.parse().ok()?,
        pat.parse().ok()?,
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReleaseVerdict {
    Current,
    UpdateAvailable { version: String, tag: String },
    Ignored,
}

pub fn evaluate_release(current: SemVer, tag: &str, draft: bool, prerelease: bool) -> ReleaseVerdict {
    if draft || prerelease {
        return ReleaseVerdict::Ignored;
    }
    let Some(latest) = parse_release_tag(tag) else {
        return ReleaseVerdict::Ignored;
    };
    if latest > current {
        ReleaseVerdict::UpdateAvailable {
            version: latest.display(),
            tag: tag.to_string(),
        }
    } else {
        ReleaseVerdict::Current
    }
}

pub fn interval_elapsed(last_check_at: Option<u64>, interval_days: u64, now_secs: u64) -> bool {
    match last_check_at {
        None => true,
        Some(last) => {
            let target = last.saturating_add(interval_days.saturating_mul(SECS_PER_DAY));
            now_secs >= target || now_secs < last
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The complete header set for an update request. Pure so tests can assert
/// the privacy contract: static UA, accept, and at most a stored ETag.
pub fn request_headers(etag: Option<&str>) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("User-Agent", USER_AGENT.to_string()),
        ("Accept", ACCEPT.to_string()),
    ];
    if let Some(tag) = etag {
        headers.push(("If-None-Match", tag.to_string()));
    }
    headers
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchOutcome {
    NotModified,
    Fetched {
        tag: String,
        draft: bool,
        prerelease: bool,
        etag: Option<String>,
    },
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckResult {
    Current,
    UpdateAvailable {
        version: String,
        tag: String,
        current: String,
    },
    Failed,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

pub fn fetch_latest_release(etag: Option<&str>) -> FetchOutcome {
    let mut request = ureq::get(RELEASES_ENDPOINT).timeout(TIMEOUT);
    for (key, value) in request_headers(etag) {
        request = request.set(key, &value);
    }
    match request.call() {
        Ok(response) if response.status() == 304 => FetchOutcome::NotModified,
        Ok(response) if response.status() == 200 => {
            let new_etag = response.header("ETag").map(str::to_string);
            let mut body: Vec<u8> = Vec::new();
            let mut taken = response.into_reader().take(MAX_BODY_BYTES);
            if taken.read_to_end(&mut body).is_err() {
                return FetchOutcome::Failed;
            }
            if (body.len() as u64) >= MAX_BODY_BYTES {
                return FetchOutcome::Failed;
            }
            match serde_json::from_slice::<GitHubRelease>(&body) {
                Ok(release) => FetchOutcome::Fetched {
                    tag: release.tag_name,
                    draft: release.draft,
                    prerelease: release.prerelease,
                    etag: new_etag,
                },
                Err(_) => FetchOutcome::Failed,
            }
        }
        Ok(_) => FetchOutcome::Failed,
        Err(_) => FetchOutcome::Failed,
    }
}

/// Applies a fetch outcome to settings and produces the user-facing result.
/// ETag is stored only when the outcome needs no action, so a later
/// `304 Not Modified` can only ever mean "still current". Failures never
/// advance `last_update_check_at`, so the next launch retries.
pub fn apply_fetch_result(
    settings: &mut UpdateSettings,
    current: SemVer,
    now: u64,
    fetched: FetchOutcome,
) -> UpdateCheckResult {
    match fetched {
        FetchOutcome::NotModified => {
            settings.last_update_check_at = Some(now);
            UpdateCheckResult::Current
        }
        FetchOutcome::Failed => UpdateCheckResult::Failed,
        FetchOutcome::Fetched {
            tag,
            draft,
            prerelease,
            etag,
        } => {
            settings.last_update_check_at = Some(now);
            match evaluate_release(current, &tag, draft, prerelease) {
                ReleaseVerdict::UpdateAvailable { version, tag } => {
                    UpdateCheckResult::UpdateAvailable {
                        version,
                        tag,
                        current: current.display(),
                    }
                }
                ReleaseVerdict::Current | ReleaseVerdict::Ignored => {
                    settings.optional_github_etag = etag;
                    UpdateCheckResult::Current
                }
            }
        }
    }
}

pub fn perform_check(
    settings: &mut UpdateSettings,
    current: SemVer,
    now: u64,
) -> UpdateCheckResult {
    let fetched = fetch_latest_release(settings.optional_github_etag.as_deref());
    apply_fetch_result(settings, current, now, fetched)
}

/// Strict charset for tags used to build a release-page URL: git-ref safe
/// characters only, bounded length, no `/`, `?`, `#`, `%` or control bytes.
pub fn validate_release_tag(tag: &str) -> Result<(), String> {
    if tag.is_empty() || tag.len() > 64 {
        return Err("release tag is empty or too long".into());
    }
    if !tag
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b'_'))
    {
        return Err("release tag contains invalid characters".into());
    }
    Ok(())
}

pub fn release_page_url(tag: &str) -> Result<String, String> {
    validate_release_tag(tag)?;
    Ok(format!("{RELEASE_PAGE_BASE}{tag}"))
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_FILE)
}

pub fn load_settings(config_dir: &Path) -> UpdateSettings {
    let path = settings_path(config_dir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return UpdateSettings::default();
    };
    let mut settings: UpdateSettings = match serde_json::from_str(&raw) {
        Ok(s) => s,
        Err(_) => return UpdateSettings::default(),
    };
    if !ALLOWED_INTERVALS_DAYS.contains(&settings.update_check_interval_days) {
        settings.update_check_interval_days = RECOMMENDED_INTERVAL_DAYS;
    }
    settings
}

pub fn save_settings(config_dir: &Path, settings: &UpdateSettings) -> Result<(), String> {
    fs::create_dir_all(config_dir).map_err(|e| format!("cannot create config directory: {e}"))?;
    let raw = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("cannot serialize update settings: {e}"))?;
    fs::write(settings_path(config_dir), raw)
        .map_err(|e| format!("cannot write update settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(maj: u64, min: u64, pat: u64) -> SemVer {
        SemVer::from_parts(maj, min, pat)
    }

    #[test]
    fn parses_normal_tags_only() {
        assert_eq!(parse_release_tag("v0.2.0"), Some(v(0, 2, 0)));
        assert_eq!(parse_release_tag("0.2.0"), Some(v(0, 2, 0)));
        assert_eq!(parse_release_tag("v10.20.30"), Some(v(10, 20, 30)));
        assert_eq!(parse_release_tag("0.2.0-rc1"), None);
        assert_eq!(parse_release_tag("0.2.0+build5"), None);
        assert_eq!(parse_release_tag("0.2"), None);
        assert_eq!(parse_release_tag("0.2.0.1"), None);
        assert_eq!(parse_release_tag("v0.2.x"), None);
        assert_eq!(parse_release_tag(""), None);
        assert_eq!(parse_release_tag("v"), None);
    }

    #[test]
    fn semantic_not_lexicographic_comparison() {
        assert!(v(0, 10, 0) > v(0, 9, 0));
        assert!(v(1, 0, 0) > v(0, 99, 0));
        assert!(v(0, 1, 10) > v(0, 1, 9));
        assert_eq!(
            evaluate_release(v(0, 9, 0), "v0.10.0", false, false),
            ReleaseVerdict::UpdateAvailable {
                version: "0.10.0".into(),
                tag: "v0.10.0".into()
            }
        );
    }

    #[test]
    fn release_filtering_rules() {
        let cur = v(0, 1, 0);
        assert_eq!(
            evaluate_release(cur, "v0.2.0", false, false),
            ReleaseVerdict::UpdateAvailable {
                version: "0.2.0".into(),
                tag: "v0.2.0".into()
            }
        );
        assert_eq!(evaluate_release(cur, "v0.1.0", false, false), ReleaseVerdict::Current);
        assert_eq!(evaluate_release(cur, "v0.0.9", false, false), ReleaseVerdict::Current);
        assert_eq!(evaluate_release(cur, "v0.2.0", true, false), ReleaseVerdict::Ignored);
        assert_eq!(evaluate_release(cur, "v0.2.0", false, true), ReleaseVerdict::Ignored);
        assert_eq!(evaluate_release(cur, "nightly", false, false), ReleaseVerdict::Ignored);
        assert_eq!(evaluate_release(cur, "v0.2.0-rc1", false, false), ReleaseVerdict::Ignored);
    }

    #[test]
    fn scheduling_interval_logic() {
        let day = SECS_PER_DAY;
        assert!(interval_elapsed(None, 7, 1_000_000));
        assert!(!interval_elapsed(Some(1_000_000), 7, 1_000_000 + 6 * day));
        assert!(interval_elapsed(Some(1_000_000), 7, 1_000_000 + 7 * day));
        assert!(interval_elapsed(Some(1_000_000), 1, 1_000_000 + day));
        assert!(!interval_elapsed(Some(1_000_000), 30, 1_000_000 + 29 * day));
        assert!(interval_elapsed(Some(u64::MAX), 7, 10));
    }

    #[test]
    fn request_headers_carry_no_identifiers() {
        let with_etag = request_headers(Some("\"abc\""));
        assert_eq!(with_etag.len(), 3);
        let keys: Vec<&str> = with_etag.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["User-Agent", "Accept", "If-None-Match"]);
        assert_eq!(with_etag[0].1, "metadata-nt-update-checker");
        assert_eq!(with_etag[1].1, "application/vnd.github+json");
        assert_eq!(with_etag[2].1, "\"abc\"");
        let without = request_headers(None);
        assert_eq!(without.len(), 2);
        for (_, value) in &without {
            assert!(!value.contains('@'), "no emails/usernames in headers");
            assert!(!value.contains("://"), "no URLs/hosts in headers");
        }
    }

    #[test]
    fn etag_only_stored_when_no_action_needed() {
        let cur = v(0, 1, 0);
        let mut s = UpdateSettings::default();
        let r = apply_fetch_result(
            &mut s,
            cur,
            500,
            FetchOutcome::Fetched {
                tag: "v0.1.0".into(),
                draft: false,
                prerelease: false,
                etag: Some("E1".into()),
            },
        );
        assert_eq!(r, UpdateCheckResult::Current);
        assert_eq!(s.optional_github_etag.as_deref(), Some("E1"));
        assert_eq!(s.last_update_check_at, Some(500));

        let mut s2 = UpdateSettings::default();
        let r2 = apply_fetch_result(
            &mut s2,
            cur,
            600,
            FetchOutcome::Fetched {
                tag: "v0.2.0".into(),
                draft: false,
                prerelease: false,
                etag: Some("E2".into()),
            },
        );
        assert_eq!(
            r2,
            UpdateCheckResult::UpdateAvailable {
                version: "0.2.0".into(),
                tag: "v0.2.0".into(),
                current: "0.1.0".into()
            }
        );
        assert_eq!(s2.optional_github_etag, None, "update pending: keep re-fetching");

        let mut s3 = UpdateSettings {
            optional_github_etag: Some("E1".into()),
            ..UpdateSettings::default()
        };
        let r3 = apply_fetch_result(&mut s3, cur, 700, FetchOutcome::NotModified);
        assert_eq!(r3, UpdateCheckResult::Current);
        assert_eq!(s3.optional_github_etag.as_deref(), Some("E1"));
        assert_eq!(s3.last_update_check_at, Some(700));

        let mut s4 = UpdateSettings::default();
        let r4 = apply_fetch_result(&mut s4, cur, 800, FetchOutcome::Failed);
        assert_eq!(r4, UpdateCheckResult::Failed);
        assert_eq!(s4.last_update_check_at, None, "failure must not consume the interval");
    }

    #[test]
    fn release_page_url_validation() {
        assert_eq!(
            release_page_url("v0.2.0").unwrap(),
            "https://github.com/ParamoStudio/metadata-nt/releases/tag/v0.2.0"
        );
        for bad in [
            "",
            "v0.2.0/../x",
            "v0.2.0?x=1",
            "v0.2.0#frag",
            "v0.2.0%2f",
            "space tag",
            &"v".repeat(65),
        ] {
            assert!(release_page_url(bad).is_err(), "must reject {bad:?}");
        }
    }

    #[test]
    fn settings_roundtrip_and_interval_sanitization() {
        let dir = std::env::temp_dir().join(format!("mnt-update-test-{}", uuid::Uuid::new_v4()));
        let s = UpdateSettings {
            onboarding_completed: true,
            automatic_update_checks_enabled: true,
            update_check_interval_days: 30,
            last_update_check_at: Some(123),
            optional_github_etag: Some("\"e\"".into()),
        };
        save_settings(&dir, &s).unwrap();
        assert_eq!(load_settings(&dir), s);
        fs::write(
            settings_path(&dir),
            "{\"onboardingCompleted\":true,\"automaticUpdateChecksEnabled\":false,\"updateCheckIntervalDays\":99,\"lastUpdateCheckAt\":null,\"optionalGithubEtag\":null}",
        )
        .unwrap();
        assert_eq!(
            load_settings(&dir).update_check_interval_days,
            RECOMMENDED_INTERVAL_DAYS
        );
        assert_eq!(
            load_settings(&dir.join("missing-dir")),
            UpdateSettings::default()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
