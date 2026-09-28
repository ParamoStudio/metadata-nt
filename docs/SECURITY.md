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
- **No HTTP plugin, no updater, no telemetry** — offline by design.
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

### Task 14 decision: external links & Reveal without the opener plugin

`external.rs` opens the three hard-coded URLs and the job's committed output
directories via Rust-side `std::process::Command::new("open")` (macOS system
opener, argv vector, never a shell). **No opener plugin, no capability change**:
the WebView has no URL-opening or path-opening permission at all; it invokes
only `open_mat2_site` / `open_dangerzone_site` / `open_privacytools_site` /
`reveal_output(job_id)`. Defense in depth: `open_approved_url` re-validates the
URL against the compile-time allow-list and rejects query/fragment; unit tests
assert exactly three URL constant declarations exist in the source and that
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
the two local dev/IPC origins in `tauri.conf.json` and the three hard-coded
external-link constants in `app/src-tauri/src/external.rs` (Task 14). Docs and
scripts directories are out of scope by design.

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
