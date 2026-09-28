//! Investigation Tripwire — Canarytokens.org Fast Redirect client.
//!
//! Owner-approved, optional, subordinate to Synthetic Metadata. This is the
//! application's ONLY intentional network path (docs/SECURITY.md §Tripwire):
//! - destination origin is hard-coded below; no other host is reachable
//!   through this module and no generic HTTP IPC exists;
//! - the request carries exactly four fields (token_type, email, memo,
//!   redirect_url) — never filenames, paths, file bytes, original metadata,
//!   synthetic profiles, hostname or account name;
//! - the alert email goes to the service ONLY (server-side notification
//!   config); it is never written into output files, never embedded in the
//!   token URL, never logged, never persisted;
//! - the returned token URL is planted as a literal string and verified by
//!   literal comparison — this module NEVER issues GET/HEAD against it and no
//!   other code path may (regression-tested in the engine suite);
//! - HTTPS with normal certificate validation (ureq/rustls), short timeout,
//!   bounded response, no cookies, no redirects, no retry loop.
//!
//! Contract verified 2026-09-28 against thinkst/canarytokens @ c1a3e87
//! (frontend/app.py api_generate + models/fast_redirect.py) and live against
//! production: form-encoded POST, JSON response, token URL in `token_url`
//! (random canary domain, http scheme — parse it, never construct it).

use std::time::Duration;

use serde::{Deserialize, Serialize};

const CANARY_ORIGIN: &str = "https://canarytokens.org";
const CREATE_PATH: &str = "/d3aece8093b71007b5ccfedad91ebb11/generate";
const TOKEN_TYPE: &str = "fast_redirect";
const MAX_BODY_BYTES: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(12);
const MEMO_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TripwireOptions {
    pub enabled: bool,
    pub email: String,
    pub redirect_url: String,
}

impl std::fmt::Debug for TripwireOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TripwireOptions")
            .field("enabled", &self.enabled)
            .field("email", &"<redacted>")
            .field("redirect_url", &self.redirect_url)
            .finish()
    }
}

#[derive(Clone)]
pub struct TripwireConfig {
    pub email: String,
    pub redirect_url: String,
}

impl std::fmt::Debug for TripwireConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TripwireConfig")
            .field("email", &"<redacted>")
            .field("redirect_url", &self.redirect_url)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct TripwireToken {
    pub token_url: String,
}

/// Redirect destination validation (spec §11): HTTPS only; file:, javascript:,
/// data: and custom app schemes are rejected by the scheme check; never
/// fetched, resolved or existence-checked.
pub fn validate_redirect_url(url: &str) -> Result<(), String> {
    if url.len() > 2048 || url.is_empty() {
        return Err("redirect destination is empty or too long".into());
    }
    if !url.starts_with("https://") {
        return Err("redirect destination must be an https:// URL".into());
    }
    let rest = &url["https://".len()..];
    if rest.is_empty() {
        return Err("redirect destination has no host".into());
    }
    if url
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || (c as u32) < 0x21 || (c as u32) > 0x7e)
    {
        return Err(
            "redirect destination contains invalid characters (printable ASCII only)".into(),
        );
    }
    Ok(())
}

/// Basic local email syntax check only (spec §12) — never probed, never
/// verified against any server beyond the token-creation call itself.
pub fn validate_email(email: &str) -> Result<(), String> {
    if email.len() > 254 || email.is_empty() {
        return Err("alert email is empty or too long".into());
    }
    if email
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || !c.is_ascii_graphic())
    {
        return Err("alert email contains invalid characters".into());
    }
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return Err("alert email must contain exactly one '@'".into());
    }
    let (local, domain) = (parts[0], parts[1]);
    if local.is_empty() || local.len() > 64 {
        return Err("alert email local part is invalid".into());
    }
    if domain.len() < 4 || !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.')
    {
        return Err("alert email domain is invalid".into());
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return Err("alert email local part is invalid".into());
    }
    Ok(())
}

/// Neutral random memo (spec §14): CSPRNG-backed, no derivation from file
/// data, names, machine or user identity; never a stable per-user identifier.
pub fn generate_memo() -> String {
    let id: String = {
        let bytes = *uuid::Uuid::new_v4().as_bytes();
        bytes
            .iter()
            .take(8)
            .map(|b| MEMO_ALPHABET[(*b as usize) % MEMO_ALPHABET.len()] as char)
            .collect()
    };
    format!("metadata'nt reference {id}")
}

pub struct TripwireRequestSpec {
    pub url: String,
    pub fields: Vec<(String, String)>,
}

/// Pure request construction — unit-tested for minimization (spec §29):
/// exactly the four contract fields, hard-coded origin, nothing else.
pub fn build_create_request(
    config: &TripwireConfig,
    memo: &str,
) -> Result<TripwireRequestSpec, String> {
    validate_email(&config.email)?;
    validate_redirect_url(&config.redirect_url)?;
    if memo.len() > 1000 || memo.is_empty() {
        return Err("memo violates the service limit".into());
    }
    Ok(TripwireRequestSpec {
        url: format!("{CANARY_ORIGIN}{CREATE_PATH}"),
        fields: vec![
            ("token_type".to_string(), TOKEN_TYPE.to_string()),
            ("email".to_string(), config.email.clone()),
            ("memo".to_string(), memo.to_string()),
            ("redirect_url".to_string(), config.redirect_url.clone()),
        ],
    })
}

#[derive(Deserialize)]
struct CreateResponse {
    token_url: Option<String>,
    error: Option<String>,
    error_message: Option<String>,
}

/// Pure response parsing (testable without network). Success requires a
/// non-empty token_url; anything else is a service error surfaced safely
/// (message sanitized, token/URL fragments never echoed in full).
pub fn parse_create_response(status: u16, body: &str) -> Result<TripwireToken, String> {
    let limited = if body.len() > MAX_BODY_BYTES as usize {
        let mut end = MAX_BODY_BYTES as usize;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        &body[..end]
    } else {
        body
    };
    let parsed: CreateResponse = serde_json::from_str(limited)
        .map_err(|e| format!("canarytokens response was not valid JSON ({e})"))?;
    if status == 200 {
        match parsed.token_url {
            Some(url) if !url.trim().is_empty() => Ok(TripwireToken { token_url: url }),
            _ => Err("canarytokens returned no token URL".into()),
        }
    } else {
        let msg = parsed
            .error_message
            .or(parsed.error)
            .unwrap_or_else(|| format!("HTTP {status}"));
        Err(format!(
            "canarytokens service error: {}",
            crate::log_sanitize::sanitize(&msg)
        ))
    }
}

/// Mask a token URL for logs/results: everything but the last four
/// characters is dropped (spec §29: logs redact the full token URL).
pub fn redact_token_url(url: &str) -> String {
    let tail: String = url
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

/// The single network entry point. POST-only, hard-coded origin, form body,
/// no redirects, no cookies, bounded response, one attempt (no retry loop).
pub fn create_investigation_tripwire(config: &TripwireConfig) -> Result<TripwireToken, String> {
    let spec = build_create_request(config, &generate_memo())?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .redirects(0)
        .build();
    let form: Vec<(&str, &str)> = spec
        .fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let result = agent.post(&spec.url).timeout(TIMEOUT).send_form(&form);
    match result {
        Ok(resp) => {
            let status = resp.status();
            let body = read_limited(resp);
            parse_create_response(status, &body)
        }
        Err(ureq::Error::Status(status, resp)) => {
            let body = read_limited(resp);
            parse_create_response(status, &body)
        }
        Err(ureq::Error::Transport(e)) => Err(format!(
            "cannot reach canarytokens.org (offline or blocked): {}",
            e.to_string().chars().take(120).collect::<String>()
        )),
    }
}

fn read_limited(resp: ureq::Response) -> String {
    use std::io::Read;
    let mut body = String::new();
    let mut limited = resp.into_reader().take(MAX_BODY_BYTES);
    let _ = limited.read_to_string(&mut body);
    body
}

/// Injection point so the pipeline is testable without live network
/// (spec §23: automated tests use mocks only).
pub type TripwireCreator = fn(&TripwireConfig) -> Result<TripwireToken, String>;

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> TripwireConfig {
        TripwireConfig {
            email: "alias@relay.example".into(),
            redirect_url: "https://archive.org/".into(),
        }
    }

    #[test]
    fn origin_and_endpoint_are_hardcoded() {
        let spec = build_create_request(&cfg(), "metadata'nt reference TESTTEST").unwrap();
        assert_eq!(
            spec.url,
            "https://canarytokens.org/d3aece8093b71007b5ccfedad91ebb11/generate"
        );
        assert!(spec.url.starts_with(CANARY_ORIGIN));
    }

    #[test]
    fn request_is_minimal_four_fields_only() {
        let spec = build_create_request(&cfg(), "metadata'nt reference AB12CD34").unwrap();
        let keys: Vec<&str> = spec.fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["token_type", "email", "memo", "redirect_url"]);
        let get = |k: &str| {
            spec.fields
                .iter()
                .find(|(kk, _)| kk == k)
                .unwrap()
                .1
                .clone()
        };
        assert_eq!(get("token_type"), "fast_redirect");
        assert_eq!(get("email"), "alias@relay.example");
        assert_eq!(get("redirect_url"), "https://archive.org/");
        // request minimization (spec §29): no file/identity material anywhere
        let blob = spec
            .fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        for forbidden in [
            "photo",
            ".jpg",
            "/Users",
            "/tmp",
            "kumareagape",
            "MAT2",
            "Make=",
            "Model=",
        ] {
            assert!(!blob.contains(forbidden), "request leaks {forbidden:?}");
        }
    }

    #[test]
    fn alert_email_in_request_but_redacted_everywhere_else() {
        let c = cfg();
        let spec = build_create_request(&c, "metadata'nt reference ZZ99ZZ99").unwrap();
        assert!(
            spec.fields
                .iter()
                .any(|(k, v)| k == "email" && *v == c.email)
        );
        let dbg = format!("{:?}", c);
        assert!(!dbg.contains(&c.email), "Debug must redact the email");
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn redirect_validation() {
        assert!(validate_redirect_url("https://archive.org/").is_ok());
        assert!(validate_redirect_url("https://example.com/a?b=c#d").is_ok());
        for bad in [
            "http://archive.org/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,x",
            "mat2wrapper://open",
            "https://",
            "",
            "https://exa mple.com",
            "https://example.com/\u{7}",
            "https://example.com/路径",
        ] {
            assert!(validate_redirect_url(bad).is_err(), "must reject {bad:?}");
        }
        let long = format!("https://a.example/{}", "x".repeat(3000));
        assert!(validate_redirect_url(&long).is_err());
    }

    #[test]
    fn email_validation_is_local_syntax_only() {
        for ok in [
            "a@b.co",
            "alias@relay.example",
            "first.last+tag@sub.domain.org",
        ] {
            assert!(validate_email(ok).is_ok(), "must accept {ok}");
        }
        for bad in [
            "",
            "noat",
            "a@b",
            "@b.co",
            "a@.co",
            "a@b.",
            "a@@b.co",
            "a b@c.de",
            "a\u{1b}[31m@b.co",
            ".a@b.co",
            "a.@b.co",
            "a..b@c.de",
        ] {
            assert!(validate_email(bad).is_err(), "must reject {bad:?}");
        }
    }

    #[test]
    fn memo_is_neutral_random_and_wellformed() {
        let a = generate_memo();
        let b = generate_memo();
        assert!(a.starts_with("metadata'nt reference "));
        assert_eq!(a.len(), "metadata'nt reference ".len() + 8);
        assert_ne!(a, b, "memo must not be a stable identifier");
        let tail = &a["metadata'nt reference ".len()..];
        assert!(tail.chars().all(|c| MEMO_ALPHABET.contains(&(c as u8))));
    }

    #[test]
    fn parse_success_response_extracts_token_url() {
        let body = r#"{"token":"qb2l4xtd1heh7zucpjcdf3293","hostname":"qb2l4xtd1heh7zucpjcdf3293.canarytokens.com","token_url":"http://canarytokens.com/about/qb2l4xtd1heh7zucpjcdf3293/payments.js","auth_token":"4ab14a095208e2d325b1ea67fc573c7a","email":"t@example.com","webhook_url":"","error":null,"error_message":null,"token_type":"fast_redirect"}"#;
        let tok = parse_create_response(200, body).unwrap();
        assert_eq!(
            tok.token_url,
            "http://canarytokens.com/about/qb2l4xtd1heh7zucpjcdf3293/payments.js"
        );
    }

    #[test]
    fn parse_error_responses_surface_safe_message() {
        let body = r#"{"error":"6","error_message":"Blocked email supplied. Please see our Acceptable Use Policy at https://canarytokens.org/nest/legal","url":"","url_components":null,"token":"","email":"","hostname":"","auth":""}"#;
        let err = parse_create_response(400, body).unwrap_err();
        assert!(err.contains("Blocked email supplied"), "{err}");

        let err2 = parse_create_response(500, "not json at all").unwrap_err();
        assert!(err2.contains("not valid JSON"), "{err2}");

        let err3 =
            parse_create_response(200, r#"{"token_url":"","error":null,"error_message":null}"#)
                .unwrap_err();
        assert!(err3.contains("no token URL"));
    }

    #[test]
    fn oversized_body_is_bounded() {
        let huge = format!(
            r#"{{"token_url":"{}", "pad":"{}"}}"#,
            "http://c.example/t",
            "A".repeat(200_000)
        );
        let r = parse_create_response(200, &huge);
        assert!(r.is_err(), "truncated body must fail parsing, not panic");

        let multibyte = format!(
            r#"{{"token_url":"http://c.example/t","pad":"{}"}}"#,
            "é".repeat(60_000)
        );
        assert!(parse_create_response(200, &multibyte).is_err());
    }

    #[test]
    fn token_url_redaction_keeps_only_last_four() {
        let url = "http://canarytokens.com/about/qb2l4xtd1heh7zucpjcdf3293/payments.js";
        let red = redact_token_url(url);
        assert_eq!(red, "…s.js");
        assert!(!red.contains("qb2l4xtd"));
        assert!(red.len() <= 8);
    }

    #[test]
    fn module_exposes_no_get_or_head_path() {
        // spec §18: the app may only POST-create; it must never GET/HEAD the
        // token URL. Needles are assembled so this test's own source does not
        // self-match via include_str!.
        let src = include_str!("tripwire.rs");
        let dot = ".";
        let get_needle = format!("{dot}get(");
        let head_needle = format!("{dot}head(");
        let post_needle = format!("{dot}post(");
        assert!(
            !src.contains(&get_needle),
            "tripwire.rs must not contain GET calls"
        );
        assert!(
            !src.contains(&head_needle),
            "tripwire.rs must not contain HEAD calls"
        );
        assert_eq!(
            src.matches(&post_needle).count(),
            1,
            "exactly one POST call site"
        );
    }
}
