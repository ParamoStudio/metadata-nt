# Security Review v1 — MAT2 Wrapper (+ Synthetic Add-on + Investigation Tripwire)

Date: 2026-09-28 · Reviewer: implementation agent (self-review; owner QA pending
per docs/TEST_PROTOCOL.md) · Build under review: main @ tripwire commit, dev
runtime = upstream 0.15.0 @ `70c17d3` (GPG-verified), frozen runtime =
`build/runtime-dist` (manifest-hashed).

This document consolidates Task 21 (network audit), Task 22 (capability /
process / filesystem audit) and Task 23 (final review matrix) so the whole
shipped surface — including the owner-approved Synthetic Metadata add-on and
Investigation Tripwire — is audited once, together. Evidence commands are
reproducible; every claim below was re-verified at review time.

---

## Section A — Network audit (Task 21)

| Check | Method | Result |
|---|---|---|
| Frontend `fetch`/XHR/WebSocket/sendBeacon/EventSource | `grep -rE` over `app/src` + `index.html`; enforced by `npm run lint:no-html-sinks` | **0 usages** (single textual match is the rule's own doc comment) |
| HTTP client crates | `cargo tree \| grep -iE 'reqwest\|hyper\|ureq\|curl\|attohttpc\|minreq'` | **ureq 2.12.1 only** — used exclusively by `tripwire.rs` |
| Remote CSS/fonts/images/scripts | `npm run lint:no-remote` (allow-list: ipc.localhost, dev URL, 7 external.rs constants, canary origin in tripwire.rs, §11 default redirect value, bare scheme literals) | **PASS** |
| Tauri plugins | Cargo.toml | `tauri-plugin-dialog` only (Rust-side; **zero** frontend permissions) — no http/updater/shell plugins |
| Analytics/crash/updater/remote config | source review + dependency list | none present |
| Telemetry | source review | none; footer "No telemetry" remains accurate |
| Offline operation | Packaging battery runs the frozen runtime under `env -i` (no Homebrew, no user env): 14 format cleans + inspect + synthetic selftest/apply + tripwire plant (unroutable 127.0.0.1:9 URL) | **PASS** — cleaning/inspection/synthetic are fully local |
| External links | `external.rs`: exactly 7 hard-coded constants (test-guarded), opened via macOS `open` argv, no query/tracking params, user-click only | **PASS** |
| Intentional network exception | `tripwire.rs`: POST-only to hard-coded `https://canarytokens.org/d3aece8093b71007b5ccfedad91ebb11/generate`; 4 fields; explicit user activation + first-use disclosure; WebView performs no networking (CSP/capabilities unchanged) | documented, gated, tested |

Spec §25 wording now reflected in SECURITY.md/THREAT_MODEL.md: *cleaning,
inspection and synthetic processing are fully local; the optional Investigation
Tripwire contacts Canarytokens.org only when explicitly enabled.*

---

## Section B — Capability / process / filesystem audit (Task 22)

### B.1 Tauri capabilities (complete granted set)

| Permission | Reason | Frontend surface | Can it be narrowed? |
|---|---|---|---|
| `core:event:default` | Rust→frontend job/log/status/result events | `listen()` in main.ts (7 event channels) | No — minimal push channel; custom commands are not capability-gated |

Single capability file (`capabilities/main.json`), explicitly enabled via
`app.security.capabilities: ["main"]`; window label `main` only. No shell,
filesystem, opener, http, dialog, notification, store or updater permissions.
CSP: `default/script/style/img/font-src 'self'`, `connect-src 'self' ipc:
http://ipc.localhost`, `frame/object/frame-ancestors/base-uri/form-action
'none'`. Verified against generated ACL (`gen/schemas/capabilities.json`).

### B.2 IPC command surface (25 commands — exact allow-list enforced by
`scripts/check-ipc-surface.sh`, negative-control tested)

Selection: `list_selection`, `remove_items`, `select_files`, `select_folder` ·
Output: `choose_output_root`, `output_root_info`, `reveal_output(job_id)` ·
Jobs: `start_clean_job`, `cancel_job`, `set_inplace_armed` · Inspection:
`inspect_selection` · Diagnostics: `runtime_diagnostics`, `synthetic_pack_info`,
`mat2_version`, `mat2_formats`, `mat2_check_dependencies`, `mat2_help` ·
Synthetic: `synthetic_preview` · Links (7 hard-coded destinations):
`open_mat2_site`, `open_dangerzone_site`, `open_privacytools_site`,
`open_canarytokens_site`, `open_canary_docs`, `open_canary_repo`,
`open_canary_audit`.

No generic `exec`/`shell`/`read_file`/`write_file`/`delete_file`/`open_url`/
`open_path`/`http_request`/`fetch_url` exists (gate rejects such names). The
frontend never receives or submits an absolute path, a URL, a tag name, a
writer argument or a token; it exchanges opaque IDs and typed enums only.

### B.3 Process creation (complete production call-site inventory)

| Site | Program | Args source | Notes |
|---|---|---|---|
| `mat2_runner.rs:205` `base_command` | pinned runtime (venv python or frozen binary) | `cli_prefix` + typed argv builders; `--` before paths | never a shell |
| `mat2_runner.rs:254` `inspect_json` | same runtime | `inspect_prefix` + file path | adapter protocol |
| `synthetic.rs:212` `request` | same runtime (`synthetic` subcommand / `-m synthetic_engine`) | fixed prefix + stdin JSON | engine spawns bundled exiftool internally via argv |
| `external.rs:104` `platform_open` | `/usr/bin/open` | 7 URL constants or job-recorded output dirs | never frontend-supplied |

`tripwire.rs` spawns nothing (ureq). Test-only spawns (ffmpeg/zip/tar/exiftool
probes) are inside `#[cfg(test)]`. No `sh -c`/`bash -c` anywhere
(grep-verified). MAT2 child cancellation kills the whole process group
(SIGTERM→SIGKILL) — no orphaned ProcessPoolExecutor workers.

### B.4 Filesystem mutation (complete production inventory, 23 sites)

| Module | Operations | Confined to |
|---|---|---|
| `jobs.rs` (14) | workspace create (0700) + cleanup; staging `fs::copy`; `copy_bytes_exclusive` (O_EXCL create, partial-removal, mode preservation); `.pre-synth`/`.pre-tripwire` snapshots + atomic `rename` restores | private temp workspace; approved output roots (planned paths only); in-place mode ONLY when session-armed + double-confirmed |
| `output.rs` (1) | component-wise `create_dir` with symlink rejection + canonical containment re-check | approved output roots |
| `mat2_runner.rs` (8) | stdout/stderr capture temp files (create/remove) | `temp_dir` only |
| `selection.rs`, `tripwire.rs`, `external.rs`, `lib.rs` | **zero** mutations | — |

Engine-side writes occur only at paths the pipeline passes (staged cleaned
files inside the private workspace); the OOXML adapter never extracts to disk
and rejects unsafe member names (ZipSlip-tested).

---

## Section C — Final review matrix (Task 23)

### C.1 Plan controls

| Control | Verdict | Evidence |
|---|---|---|
| Upstream snapshot integrity | **PASS** | `verify-upstream.sh` (GPG good-signature, tree hash); re-run at every packaging |
| Upstream tests | **PASS** | 147 tests, 146 pass (1 ffmpeg-version-pinned mp4 expectation, environmental; 1 upstream skip) — identical at Task 1 and Task 20 reruns |
| No modified MAT2 sanitisation | **PASS** | upstream tree pristine (git-verified); frozen bundle prunes only tests/CI/desktop-integration; runtime entry calls upstream's own functions (documented process-model deviation, zero logic change) |
| Strict CSP | **PASS** | §B.1 |
| Minimal capabilities | **PASS** | `core:event:default` only |
| No generic shell | **PASS** | §B.3 |
| No generic filesystem IPC | **PASS** | §B.2 (opaque IDs; Rust owns all paths) |
| No generic URL opener | **PASS** | 7 constants, test-guarded count |
| No document previews | **PASS** | no thumbnail/viewer code; grep-verified |
| Hostile filename tests | **PASS** | 13-name argv battery + E2E (`--version.jpg`, `$(touch owned)`, ANSI, Unicode, quotes) — no injection artifacts created |
| Symlink/path confinement | **PASS** | enumeration never follows symlinks; `..` rejected; symlinked output components rejected; containment invariant test |
| False-success tests | **PASS** | exit-0-no-output ⇒ Failed; unverifiable ⇒ Failed; remaining metadata ⇒ Warning; show-mode exit-0 trap documented+tested |
| Output collision tests | **PASS** | `.cleaned-2/-3` ladder; pre-planted symlinks count as occupied; O_EXCL commit |
| Destructive-mode confirmation tests | **PASS** | arm-gate (session memory, consumed per job), double modal, output-routing lock, stale-state rejection; QA Test E covers restart |
| No telemetry | **PASS** | §A |
| Offline operation | **PASS** | §A (env -i battery); tripwire is the single documented exception |
| Runtime dependency diagnostics | **PASS** | fail-closed gating (PROCESS disabled), fatal/non-fatal split |
| Post-clean MAT2 verification | **PASS** | exists+regular+non-empty+structured post-inspection before commit; green requires all |
| Original unchanged in normal mode | **PASS** | SHA-256 assertions across pipeline/format/security suites |
| Third-party license inclusion | **PASS** | `docs/THIRD_PARTY_LICENSES.md` (Rust/npm/frozen bundle/services + GPL aggregation note) |

### C.2 Synthetic add-on controls

| Control | Verdict | Evidence |
|---|---|---|
| Off by default; only on verified MAT2-cleaned staged files | **PASS** | pipeline order + tests |
| Coherent profiles; sparsity; semantic fields never | **PASS** | engine suite (24 generator/validator tests) |
| Typed backend-generated writer args only | **PASS** | §B.2/B.3; `test_writer_args_are_shell_free_and_dashed_safe` |
| Original values never survive/reappear | **PASS** | baseline-aware absence verification; `no_real_identifiers` |
| Synthetic failure falls back to clean output (no false success) | **PASS** | snapshot/restore + Warning copy tests |
| Pack integrity | **PASS** | SHA-256 pinned in script + Rust; schema validation at job start |
| Recipe reality (§19) | **PASS** | every Tier-1 tag write+read-back proven vs pinned exiftool 13.55 |

### C.3 Investigation Tripwire controls (spec §27 expectations)

| Expectation | Verdict |
|---|---|
| No background networking | **PASS** — creation happens only inside a user-started job with tripwire enabled |
| No frontend networking | **PASS** — §A/§B.1 (CSP/capabilities unchanged) |
| No telemetry / no updater polling / no remote assets | **PASS** — §A |
| MAT2 cleaning works offline | **PASS** — env -i battery |
| Synthetic metadata works offline | **PASS** — env -i battery (incl. plant with unroutable URL) |
| Tripwire is the only intentional network path | **PASS** — §A (ureq sole HTTP crate; single origin constant) |
| Request requires explicit user activation | **PASS** — OFF by default; requires Synthetic ON; first-use disclosure per session; Enable-Tripwire confirmation gates submission |
| Destination origin hard-coded | **PASS** — `origin_and_endpoint_are_hardcoded` |
| No file content sent | **PASS** — `request_is_minimal_four_fields_only` |
| No filename/path/original metadata sent | **PASS** — same test (forbidden-substring assertions) |
| Alert email not written into output metadata | **PASS** — engine email-absence test + pipeline file/log/result assertions |
| Returned Canary URL never fetched by the app | **PASS** — `module_exposes_no_get_or_head_path` + mandatory `test_tripwire_url_is_never_fetched_during_write_or_verify` (engine AST scan) |
| Tripwire unavailable when Synthetic OFF | **PASS** — UI structure + backend neutralisation (incl. in-place) |

### C.4 Release-blocking failures

**None.** Open items explicitly deferred to owner (not blockers):
1. Product license decision (first-party crate carries `LicenseRef-Proprietary`
   placeholder).
2. App icon replacement (Tauri placeholders) and signing/notarization for
   public distribution (Task 24 note).
3. Manual QA execution (TEST_PROTOCOL Tests A–I, incl. the single live
   sacrificial-token test — the only step that touches the real service).
4. m4a synthetic writer has no upstream fixture — deferred pending a test
   file (recipe-gated; reports unavailable cleanly until proven).
