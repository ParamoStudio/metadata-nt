# MAT2 Wrapper — Security Policy & Permission Ledger

## Gate rule (IMPLEMENTATION_PLAN Task 3)

> Feature implementation must not broaden permissions casually. **Any new
> permission requires a one-line rationale in this file, added in the same
> commit as the permission.**

## Tauri capability ledger

Capability file: `app/src-tauri/capabilities/main.json` (the only one; explicitly
enabled via `app.security.capabilities: ["main"]` in `tauri.conf.json` so no
other capability file can be silently auto-enabled).

| Permission | Reason | Frontend surface using it | Can it be narrowed? |
|---|---|---|---|
| `core:event:default` | Rust→frontend job/log/status events (listen/emit) | Job progress + technical log rendering (Task 10–12) | No — event listen/emit is the minimum IPC push channel; custom commands are not capability-gated |

Nothing else is granted. In particular:

- **No shell plugin** — process creation happens only in Rust, only for MAT2
  and its runtime diagnostics (`std::process::Command`, argv vectors, never a shell).
- **No filesystem plugin** — all file access is through semantic Rust commands
  bound to the approved-path registry (opaque IDs at the boundary).
- **No generic opener** — the three external links become dedicated Rust
  commands with hard-coded URL constants (Task 14); `open_url(url)` /
  `open_path(path)` never exist.
- **No HTTP plugin, no auto-updater, no telemetry.** Cleaning, inspection and
  synthetic metadata processing are fully local. There are exactly two
  intentional network paths, both Rust-side and both explicit: the optional
  Investigation Tripwire (Canarytokens.org, below) and the optional GitHub
  release checker (`updates.rs`, below). All file inspection, cleaning and
  synthetic metadata processing remain local. If enabled by the user,
  metadata'nt may contact GitHub to check for new application releases. This
  reveals the user's IP address to GitHub but sends no files, document
  metadata, machine identifier or usage analytics. Investigation Tripwire
  contacts Canarytokens.org only when explicitly enabled for that feature.
  Nothing is ever downloaded or installed automatically.
- `tauri-plugin-log` (present in the scaffold template) was **removed** in
  Task 2: logs are in-memory only (HANDOFF §18); no persistent log files that
  could retain sensitive filenames.

### Rust-side dependencies without frontend permissions (Task 4)

| Dependency | Rationale | Frontend permission granted |
|---|---|---|
| `tauri-plugin-dialog` | Native file/folder pickers, invoked **only from Rust** (`DialogExt`) inside `select_files`/`select_folder` commands; dialog results are registered directly in the Rust registry so absolute paths never round-trip through the WebView. Drag-and-drop arrives via Rust `WindowEvent::DragDrop`, likewise. | **None** — no `dialog:*` permission in any capability; the WebView cannot open dialogs or fabricate picker results itself |
| `uuid` (v4) | Opaque, unguessable selection IDs; keeps the frontend unable to enumerate or construct registry keys | None |
| `time` (0.3, local-offset/formatting/macros) | `YYYY-MM-DD_HHmmss` output-directory timestamps in local time; pure-Rust, no network/parser surface, replaces hand-rolled civil-calendar code | None |
| `sha2` (0.10) | SHA-256 checksums proving "source unchanged" and "staging byte-identical" invariants (Task 8 tests + QA protocol); RustCrypto, pure Rust, no network | None |
| `libc` (0.2, unix only) | Process-group signalling (`kill(-pgid, SIGTERM/SIGKILL)`) for controlled cancellation of MAT2 children, which internally spawn ProcessPoolExecutor workers; already an indirect dependency of the Rust std/tauri tree — no new supply chain | None |
| `ureq` (2.x, rustls) | Investigation Tripwire: create a Fast Redirect canary at the hard-coded Canarytokens.org origin (spec §16–17: HTTPS-only, normal cert validation, 12s timeout, 64KB response bound, 0 redirects, no cookies, POST-only, no retry loop); and the opt-in GitHub release checker (`updates.rs`: HTTPS-only GET to the hard-coded `api.github.com` releases endpoint, normal cert validation, 8s timeout, 256KB response bound, no cookies, no auth token, static User-Agent, optional stored ETag). std has no HTTP/TLS; ureq is the minimal maintained blocking client (no async runtime). Pulls rustls/webpki (allow-listed by cargo-deny) | None — Rust-side only; no generic HTTP IPC command exists; WebView keeps zero network capability |

Registry policy (selection.rs, covered by unit tests): frontend-supplied data is
only ever opaque IDs; unknown IDs resolve to nothing; duplicates dedupe by
canonical path; folder enumeration never follows symlinks; symlinked files are
classified explicitly; broken symlinks are skipped with a reason.

Planned additions (each lands with its rationale line here in the same commit):

- (none pending — decisions below resolved Task 14 without new permissions)

### Synthetic metadata add-on (owner-approved scope extension)

Owner decision supersedes the v1 non-goal "no metadata spoofing"; the add-on is
implemented per `addon-fauxmeta/SYNTHETIC_METADATA_HANDOFF.md` with these
enforced properties:

| Property | Enforcement | Verified by |
|---|---|---|
| MAT2 remains the only sanitiser | Synthetic stage runs ONLY after Phase-A clean verification, on the staged cleaned file; never on originals; never in in-place mode (command-level rejection + run_job neutralisation) | `run_job_inplace_neutralizes_synthetic`, pipeline order in `clean_one_tracked` |
| Clean output survives synthetic failure | `.pre-synth` snapshot + atomic `rename` restore; result becomes Warning "MAT2 cleaning succeeded… Clean output is available.", never false success | `run_job_synthetic_engine_failure_keeps_clean_output` |
| No frontend-controlled writer arguments | Frontend submits only the typed 6-field `SyntheticOptions`; every exiftool/mutagen/OOXML argument is engine-constructed from validated profiles; engine invoked as fixed program + argv + stdin JSON, never a shell | `options_serialize_for_frontend_and_engine`, `test_writer_args_are_shell_free_and_dashed_safe`, hostile-filename tests |
| Pack integrity | `synthetic_metadata_profiles_v1.json` SHA-256 pinned in Rust (`EXPECTED_PACK_SHA256`) AND in the packaging script; schema validation at every job start (`validate_pack`); semantic-field scrub with recorded warnings | `dev_runtime_resolves_and_pack_is_pinned`, `test_validate_pack`, packaging Stage 2/4 |
| CSPRNG, no persistent identity | Job seed = 2× UUIDv4 (getrandom-backed), memory-only, per-job; per-file derivation sha256(seed‖selection_id); never embedded in output, never logged | `profile_provenance_internal_only` validator check + `preview_has_no_seed_or_ids` |
| No semantic claims | Title/Subject/Keywords/Artist/Album/Composer/Copyright/Publisher hard-filtered at pack load and re-checked at generation and validation | `test_no_semantic_candidate_fields_survive`, `test_semantic_fields_never_generated`, `semantic_fields_off` validator |
| Original values never reused/reappear | Generator rejects profiles reusing original fragments; verifier re-reads output and fails on reappearance (baseline-aware: structural MAT2 survivors excluded by key denylist, EXIF-spec constants excluded) | `test_validator_rejects_original_value_reuse`, `test_original_values_never_reappear_guard` |
| GPS/serial off by default | Defaults enforced in pack validation (`synthetic_mode/serial_mode/location_mode` must ship off) and in Rust `SyntheticOptions::default` | `test_malformed_pack_rejected`, `defaults_are_private` |
| No second exiftool/mutagen copy | Writers use the runtime's bundled exiftool (PATH-injected) and bundled mutagen — same pinned copies MAT2 uses | packaging manifest, frozen battery |
| OOXML adapter safety | Memory-only archive rewrite (no extraction to disk), unsafe member names rejected (ZipSlip defense), payload bytes copied verbatim | `test_zipslip_member_rejected`, `test_docx_core_properties_only` |
| Recipe reality | Every Tier-1 candidate tag proven writable+readable with the pinned exiftool at test time | `test_tier1_candidate_tags_writable` (HANDOFF §19) |

New commands (allow-list updated, 21 total): `synthetic_preview` (ephemeral
seed, display data only, extension validated `[a-z0-9]{1..8}`),
`synthetic_pack_info` (diagnostics). No new Tauri permissions; capability set
unchanged (`core:event:default` only).

### Investigation Tripwire (owner-approved; intentional network path 1 of 2)

Sub-feature of Synthetic Metadata (hidden/disabled when synthetic is OFF;
can never run in in-place mode). Mints a Canarytokens.org **Fast Redirect**
token per output file and plants it as `XMP-dc:Source` on the already
synthetic-verified staged file.

| Property | Enforcement | Verified by |
|---|---|---|
| Hard-coded destination | `CANARY_ORIGIN`/`CREATE_PATH` constants in `tripwire.rs`; no URL parameter exists anywhere in the API; contract verified against thinkst/canarytokens @ c1a3e87 + live production | `origin_and_endpoint_are_hardcoded` |
| Request minimization | exactly 4 form fields: `token_type=fast_redirect`, `email`, `memo`, `redirect_url` — no filename/path/bytes/metadata/profile/hostname/username possible | `request_is_minimal_four_fields_only` |
| Alert-email containment | email goes ONLY to the service (server-side notification config); never in file metadata, token URL, memo, logs, results or IPC; `Debug` impls redact it | `alert_email_in_request_but_redacted_everywhere_else`, engine email-absence test, pipeline email-privacy assertions |
| Token URL never fetched | module has zero GET/HEAD call sites (static test); engine has zero network imports (AST test); verification is literal string comparison only; CI/QA never uses live tokens | `module_exposes_no_get_or_head_path`, `test_tripwire_url_is_never_fetched_during_write_or_verify` (mandatory spec §18 name) |
| Redirection | full URL redacted to `…last4` before IPC/logs/results | `token_url_redaction_keeps_only_last_four`, `run_job_tripwire_planted_verified_and_redacted` |
| Neutral memo | `metadata'nt reference <8 CSPRNG chars>`; no derivation from file/machine/user; never stable | `memo_is_neutral_random_and_wellformed` |
| Redirect destination | `https://` only; `file:`/`javascript:`/`data:`/custom schemes rejected; printable-ASCII bound; never fetched/resolved; default `https://archive.org/` | `redirect_validation` |
| One token per file, sequential | created inside the sequential per-file pipeline; no concurrency | pipeline structure + `run_job_tripwire_*` tests |
| Failure isolation (§20) | creation failure → synthetic still applied, Warning, "Clean output is available"; plant/verify failure → `.pre-tripwire` snapshot atomically restored (synthetic survives); offline → graceful Transport error | `run_job_tripwire_creation_failure_keeps_synthetic_output`, engine `failed_kept_synthetic` test, packaging battery (unroutable 127.0.0.1:9 URL) |
| Explicit activation | OFF by default every launch; requires Synthetic ON; first-use network disclosure per session (session-only, never persisted); Enable-Tripwire confirmation gates job submission | frontend session flags (nothing stored) + QA Test I |
| WebView unchanged | networking is Rust-only; CSP/capabilities untouched (no `connect-src` additions, no HTTP IPC command) | lint gates + capability ledger above |

Commands added (allow-list 25 total): `open_canarytokens_site`,
`open_canary_docs`, `open_canary_repo`, `open_canary_audit` — same hard-coded
constant mechanism as the original three (now 7 approved URLs; the
exactly-seven test guards the set).

### GitHub update checker (owner-approved; intentional network path 2 of 2)

Release checker only — no download, no installation, no Sparkle/updater
framework, no background daemon, no polling timer. Checks
`ParamoStudio/metadata-nt` stable releases via the hard-coded GitHub Releases
API; the bundled MAT2 runtime is never updated independently.

| Property | Enforcement | Verified by |
|---|---|---|
| Hard-coded endpoint | `RELEASES_ENDPOINT` constant (`api.github.com/repos/ParamoStudio/metadata-nt/releases/latest`); no URL/repository parameter exists in any command | `request_headers_carry_no_identifiers`, IPC surface lint |
| Request minimization | static User-Agent `metadata-nt-update-checker`, Accept header, optional stored ETag only; no cookies, no auth token, no machine/installation identifier, no hostname, no username, no file or document data | `request_headers_carry_no_identifiers` |
| Consent-gated automatic checks | first-run onboarding requires an explicit choice (dismissing ≠ consent); automatic checks run only at launch and only when the user-selected interval (1/7/30 days, default 7) has elapsed; manual checks are always explicit | `scheduling_interval_logic`, frontend onboarding gate, QA protocol |
| Silent failure policy | automatic check: no-update and network failure produce no UI; manual check always reports (up to date / could not check / update available) | frontend result dialog routing, QA protocol |
| Stable releases only | drafts, prereleases, malformed and non-semver tags rejected; numeric semantic comparison (0.10.0 > 0.9.0), never lexicographic | `release_filtering_rules`, `semantic_not_lexicographic_comparison`, `parses_normal_tags_only` |
| No auto-download/install | the check returns display data only; `View Release` opens the official release page constructed locally from a strictly validated tag via `external::open_release_page` (official-prefix + no-query/fragment re-check) | `release_page_url_validation`, `open_release_page` guard |
| ETag conditional requests | ETag stored only when the outcome needs no action, so `304 Not Modified` can only mean "still current"; failures never consume the interval | `etag_only_stored_when_no_action_needed` |
| Local state | one JSON file in the app config dir holding exactly: `onboarding_completed`, `automatic_update_checks_enabled`, `update_check_interval_days`, `last_update_check_at`, `optional_github_etag`; no unique identifier, no telemetry, no update history | `settings_roundtrip_and_interval_sanitization` |
| Tripwire isolation | separate module, separate origin, separate user action; no shared data, no request chaining | module boundaries + this table |

Commands added (allow-list 30 total): `update_settings_get`,
`update_settings_set`, `update_check_now`, `update_auto_check_if_due`,
`open_update_release`. No new Tauri permissions; CSP unchanged (networking
stays Rust-side; `connect-src` remains IPC-only). The native macOS app menu
gains `Check for Updates…` and `Settings…`; both emit events handled by the
same frontend flows as the Settings UI — no duplicate update implementation.

### Task 14 decision: external links & Reveal without the opener plugin

`external.rs` opens the three hard-coded URLs and the job's committed output
directories via Rust-side `std::process::Command::new("open")` (macOS system
opener, argv vector, never a shell). **No opener plugin, no capability change**:
the WebView has no URL-opening or path-opening permission at all; it invokes
only `open_mat2_site` / `open_dangerzone_site` / `open_privacytools_site` /
`reveal_output(job_id)`. Defense in depth: `open_approved_url` re-validates the
URL against the compile-time allow-list and rejects query/fragment; unit tests
assert exactly seven URL constant declarations exist in the source and that
unapproved URLs are rejected without spawning. `reveal_output` resolves only
paths the job pipeline itself recorded, keyed by matching job id — unknown ids
error out.

## CSP (tauri.conf.json → app.security.csp)

| Directive | Value | Why |
|---|---|---|
| `default-src` | `'self'` | local bundled assets only |
| `script-src` | `'self'` | no inline script; Tauri auto-appends hashes/nonces for its own injected boot code at compile time |
| `style-src` | `'self'` | bundled CSS only; no `'unsafe-inline'` — styling via classes, not style attributes |
| `img-src` | `'self'` | app icons/assets only; user documents are NEVER rendered (no previews), so no `asset:`/`blob:`/`data:` needed |
| `font-src` | `'self'` | system fonts; no remote fonts |
| `connect-src` | `'self' ipc: http://ipc.localhost` | only the Tauri IPC channel; no network endpoints exist |
| `frame-src` / `object-src` / `frame-ancestors` | `'none'` | no iframes, no plugins/embeds, no embedding of the app |
| `base-uri` / `form-action` | `'none'` | no base-tag hijack, no form exfiltration |

## Remote-reference lint

`scripts/check-no-remote-refs.sh` (npm: `lint:no-remote`) fails the build if any
`http(s)://` or `ws(s)://` reference appears in app source, except the allow-list:
the two local dev/IPC origins in `tauri.conf.json`, the hard-coded
external-link constants and the validated release-page prefix in
`app/src-tauri/src/external.rs` (Task 14 + update checker), the Canarytokens
origin in `tripwire.rs`, and the GitHub releases endpoint plus release-page
prefix in `updates.rs`. Docs and scripts directories are out of scope by design.

## Process-creation policy

Every `Command::new` call site lives in `mat2_runner.rs`/runtime-management code,
takes argv vectors (no shell interpolation), and is documented in the Task 22
audit table. Filenames are always passed after `--`.

## Supply-chain review (Task 20)

Full component/license inventory and per-dependency justification:
`docs/THIRD_PARTY_LICENSES.md`. Gate status:

- `cargo fmt --check` clean; `cargo clippy --all-targets` **0 warnings**
  (dead code resolved: test-only APIs `#[cfg(test)]`-gated, unused fields
  removed rather than allowed);
- `cargo audit`: 0 vulnerabilities. Two allowed warnings (RUSTSEC-2024-0370
  proc-macro-error, RUSTSEC-2024-0429 glib) are **Linux-GTK-chain /
  build-lineage only** — proven absent from the aarch64-apple-darwin graph;
- `cargo deny check` (deny.toml scoped to the shipping target): advisories,
  bans, licenses, sources all **ok**; first-party crate carries
  `LicenseRef-Proprietary` pending the owner's license decision;
- `npm audit`: 0 vulnerabilities;
- Upstream MAT2 suite rerun in dev env: 147 tests, 1 failure + 1 skip —
  identical to the Task 1 recorded baseline (environmental, not a regression);
  upstream tree restored pristine (`verify-upstream.sh` OK);
- Frozen runtime bundle: every file hashed in `MANIFEST.sha256` (964+ files),
  component versions pinned in `PACKAGE_INFO.json`, synthetic profile pack
  SHA-256 pinned in both the packaging script and Rust (`EXPECTED_PACK_SHA256`).

## Known residual risks (accepted for v1)

1. No OS-level sandbox around MAT2 child processes on macOS (upstream removed
   bubblewrap in 0.14.0; it was Linux-only anyway). Exposure equals invoking
   MAT2 directly from a terminal — the wrapper adds no new parser surface.
2. Staging/workspace deletion is not secure erase on SSD/APFS (documented in UI
   docs and Task 8).
3. MAT2 `--show` output parsing depends on upstream output shape; a format
   change would degrade the diff view (fails visible, never false-success).
