# MAT2 Tauri Wrapper — Closed Implementation Plan

> **Execution instruction for the coding agent:** implement this plan task-by-task. Use test-driven development where practical. Do not redesign the product while executing. If the supplied MAT2 source differs materially from assumptions here, document the difference and update the wrapper mapping; do not patch upstream merely to fit this plan.

## Goal

Deliver a small offline Tauri 2 desktop application, macOS-first, that wraps the supplied MAT2 source/runtime with safe file selection, batch processing, before/after inspection, output verification and a minimal attack surface.

## Architecture

A static local HTML/CSS/vanilla-TypeScript frontend communicates through a narrow typed Tauri IPC boundary to a Rust core. Rust owns approved paths, job state and `std::process::Command` invocations. MAT2 remains the only metadata sanitisation engine.

## Tech stack

- Tauri 2
- Rust
- HTML
- CSS
- vanilla TypeScript
- minimal Vite build tooling if required
- supplied MAT2 Python project/runtime
- native OS dialogs
- system browser for three hard-coded external links

Read first:

```text
HANDOFF.md
INTERFACE.md
supplied MAT2 README.md
supplied MAT2 INSTALL.md
supplied MAT2 CHANGELOG.md
supplied MAT2 CLI entrypoint
supplied MAT2 tests/
```

---

# Global constraints

1. Do not modify MAT2 sanitisation logic.
2. No alternate cleaning engine.
3. No generic shell execution.
4. No `sh -c`, `bash -c`, `zsh -c` or equivalent.
5. No user-document preview/thumbnail parsing.
6. No network required for application operation.
7. No telemetry.
8. No updater in v1.
9. No remote assets.
10. No persistent processing history/log by default.
11. Normal mode must preserve originals.
12. Every green success must be post-verified.
13. MAT2's actual supplied CLI is the authority for option parity.
14. Keep npm/Rust dependency count as low as practical.
15. macOS is the first release target.
16. Windows is not advertised until the full MAT2 runtime is validated there.

---

# Review focus

These are the five failure classes reviewers should treat as release-critical:

1. **Hostile filename / argument injection** — filenames beginning with `-`, containing quotes, shell syntax, Unicode or control characters must not alter invocation semantics.
2. **Path escape / symlink behavior** — input enumeration and output commit must never escape approved roots through `..`, symlinks or crafted relative paths.
3. **False success** — process exit alone must never yield green status; missing, invalid or unverifiable output is failure/warning as specified.
4. **Frontend compromise blast radius** — the WebView must not possess generic shell, arbitrary filesystem or arbitrary URL-opening powers.
5. **Accidental network/supply-chain behavior** — built application loads no remote content, performs no telemetry/update check and ships a verifiable MAT2/runtime snapshot.

---

# Planned repository shape

Create the wrapper **beside** the supplied MAT2 tree unless the owner explicitly wants it inside that repository.

Recommended:

```text
project-root/
├── upstream-mat2/              # supplied, do not modify cleaning code
├── app/
│   ├── package.json
│   ├── package-lock.json
│   ├── index.html
│   ├── src/
│   │   ├── main.ts
│   │   ├── state.ts
│   │   ├── types.ts
│   │   └── styles.css
│   └── src-tauri/
│       ├── Cargo.toml
│       ├── Cargo.lock
│       ├── build.rs
│       ├── tauri.conf.json
│       ├── capabilities/
│       │   └── main.json
│       └── src/
│           ├── main.rs
│           ├── model.rs
│           ├── selection.rs
│           ├── mat2_runner.rs
│           ├── jobs.rs
│           ├── output.rs
│           ├── log_sanitize.rs
│           └── external.rs
├── docs/
│   ├── UPSTREAM_SNAPSHOT.md
│   ├── THREAT_MODEL.md
│   ├── SECURITY.md
│   ├── TEST_PROTOCOL.md
│   ├── SECURITY_REVIEW_V1.md
│   └── THIRD_PARTY_LICENSES.md
└── scripts/
    ├── verify-upstream.sh
    └── package-mat2-macos.sh
```

If the owner places the wrapper inside the supplied repository instead, keep all wrapper files under a clearly separate top-level directory and do not restructure `libmat2/`, `tests/`, or upstream entrypoints.

---

# Task 1 — Establish and record the upstream snapshot

## Deliverable

A verified description of the exact MAT2 source handed to the agent.

## Steps

- [ ] Inspect repository status, remote, commit and tag.

Run:

```bash
git -C upstream-mat2 status --short
git -C upstream-mat2 remote -v
git -C upstream-mat2 rev-parse HEAD
git -C upstream-mat2 describe --tags --always --dirty
git -C upstream-mat2 log -1 --show-signature
```

Expected: values are recorded, not guessed.

- [ ] Read upstream `README.md`, `INSTALL.md`, `CHANGELOG.md`, `pyproject.toml`, CLI entrypoint, `libmat2/` and test structure.

- [ ] Run upstream tests in the documented development environment.

Use the upstream-prescribed command. At the time of handoff this has been:

```bash
python3 -m unittest discover -v
```

Expected: record actual result.

- [ ] Run and capture actual CLI diagnostics:

```bash
mat2 --help
mat2 --version
mat2 --list
mat2 --check-dependencies
```

Use the supplied entrypoint/runtime rather than an unrelated global MAT2.

- [ ] Create `docs/UPSTREAM_SNAPSHOT.md` with the exact outputs and environment notes.

- [ ] Create `scripts/verify-upstream.sh` that checks the expected commit/tree state without modifying it.

- [ ] Commit wrapper-side documentation only.

### Gate

Do not scaffold the product until option mapping and runtime expectations are known from the supplied source.

---

# Task 2 — Scaffold the smallest Tauri application

## Deliverable

One local dark window with no remote content and no product behavior yet.

## Steps

- [ ] Create Tauri 2 application under `app/`.

- [ ] Use vanilla TypeScript + HTML + CSS.

Do not add React/Vue/Svelte/Tailwind/Bootstrap.

- [ ] Remove sample commands, sample links and sample remote assets.

- [ ] Configure one main window.

Initial target:

```text
1040 × 660
minimum ~900 × 560
```

- [ ] Add dark static shell matching `INTERFACE.md`.

- [ ] Verify dev build opens with networking disabled.

- [ ] Commit.

### Tests/checks

- application launches;
- no console errors;
- no external requests in network inspection;
- package dependency tree is recorded.

---

# Task 3 — Establish Tauri security baseline before feature work

## Deliverable

A constrained WebView/backend boundary.

## Steps

- [ ] Read current Tauri security/capabilities/CSP docs.

- [ ] Create `src-tauri/capabilities/main.json`.

- [ ] Explicitly enable only the capability file(s) intended for the main window.

- [ ] Do not install a generic shell plugin.

- [ ] Add strict CSP with local assets only.

At minimum reason about:

```text
default-src 'self'
script-src bundled/self only
connect-src only what Tauri IPC requires
frame-src 'none'
object-src 'none'
```

Use current Tauri-required IPC sources rather than blindly copying an obsolete CSP.

- [ ] Ensure no remote capability configuration.

- [ ] Create `docs/THREAT_MODEL.md` from `HANDOFF.md`.

- [ ] Add a test/lint check that rejects accidental `http://` / `https://` references in app source except the three approved external-link constants and documentation.

- [ ] Commit.

### Gate

Feature implementation must not broaden permissions casually. Any new permission requires a one-line rationale in `docs/SECURITY.md`.

---

# Task 4 — Define backend models and opaque selection registry

## Files

Create/modify:

```text
app/src-tauri/src/model.rs
app/src-tauri/src/selection.rs
app/src-tauri/src/main.rs
app/src/types.ts
```

## Required interfaces

Rust conceptual types:

```rust
struct SelectionId(/* opaque id */);

struct SelectedFile {
    id: SelectionId,
    path: PathBuf,          // Rust only
    display_name: String,
    extension: Option<String>,
    relative_path: Option<PathBuf>,
    size: u64,
}

struct PublicSelectedFile {
    id: String,
    display_name: String,
    extension: Option<String>,
    relative_path: Option<String>,
    size: u64,
    status: FileStatus,
}
```

Do not expose absolute source paths to frontend unless a later requirement demonstrably needs them.

## Tests first

- [ ] unknown ID cannot resolve to a path;
- [ ] registry owns only user-selected/enumerated approved paths;
- [ ] duplicate addition behaves deterministically;
- [ ] display names containing HTML-like text remain data;
- [ ] symlink classification is explicit.

## Implementation

- [ ] Add native multi-file picker.

- [ ] Add native folder picker.

- [ ] Enumerate regular files without following symlinked directories.

- [ ] Group public results by extension.

- [ ] Commit.

---

# Task 5 — Discover and map the actual MAT2 CLI

## Deliverable

A typed `Mat2Runner` whose argument construction exactly matches the supplied upstream.

## File

```text
app/src-tauri/src/mat2_runner.rs
```

## Principle

Use:

```rust
std::process::Command
```

Never a shell string.

## Required conceptual API

Adapt names to actual upstream behavior while keeping semantics:

```rust
Mat2Runner::version()
Mat2Runner::help()
Mat2Runner::list_formats()
Mat2Runner::check_dependencies()
Mat2Runner::inspect(path, options)
Mat2Runner::clean(path, options)
```

Typed processing options should represent only real upstream capabilities discovered in Task 1.

## Tests first

Construct tests for:

- [ ] normal cleaning;
- [ ] lightweight;
- [ ] inspection/show;
- [ ] verbose;
- [ ] unknown-member policy values if present;
- [ ] in-place if present;
- [ ] version/list/dependency/help diagnostics;
- [ ] filenames beginning with `-`;
- [ ] spaces;
- [ ] single/double quotes;
- [ ] shell metacharacters;
- [ ] Unicode;
- [ ] newlines/control characters in names where platform permits.

### Critical assertion

Process invocation must never become a shell program plus concatenated string.

Where the supplied MAT2 argparse supports `--` as end-of-options, use it before file arguments. If it does not, do not assume; inspect/verify and create a safe alternate strategy.

- [ ] Implement minimal runner.

- [ ] Commit.

---

# Task 6 — Implement process-output sanitisation

## File

```text
app/src-tauri/src/log_sanitize.rs
```

## Tests first

Input containing:

- ANSI escapes;
- terminal title escapes;
- carriage-return tricks;
- backspace;
- embedded NUL;
- ordinary newline;
- Unicode.

Expected:

- preserve readable text;
- normalize line endings;
- remove unsafe controls;
- never emit executable markup;
- never create links automatically.

- [ ] Implement sanitiser.

- [ ] Ensure frontend inserts every log/message through text APIs, not `innerHTML`.

- [ ] Commit.

---

# Task 7 — Implement output planning and path confinement

## File

```text
app/src-tauri/src/output.rs
```

## Required behaviors

Default:

```text
<source-parent>/MAT2 Output/YYYY-MM-DD_HHmmss/
```

Custom:

```text
<custom-root>/YYYY-MM-DD_HHmmss/
```

Collision:

```text
name.cleaned.ext
name.cleaned-2.ext
name.cleaned-3.ext
```

## Tests first

- [ ] same-parent sources;
- [ ] multi-parent batch;
- [ ] custom root;
- [ ] nested folder relative paths;
- [ ] `..` path components;
- [ ] symlinked output components;
- [ ] collision;
- [ ] read-only target;
- [ ] Unicode paths.

Every resolved final path must remain inside its approved output root.

- [ ] Implement output planner.

- [ ] Commit.

---

# Task 8 — Implement staging and atomic commit

## Files

```text
app/src-tauri/src/jobs.rs
app/src-tauri/src/output.rs
```

## Normal-mode pipeline

```text
source
  ↓
private temp job directory
  ↓
byte-identical staging file
  ↓
MAT2 cleans staged file
  ↓
validate produced file
  ↓
post-inspect
  ↓
atomic move/rename into final destination where feasible
```

## Requirements

- workspace unique per job;
- permissions limited to current user where platform APIs allow;
- do not interpret file contents while copying;
- do not use MAT2 in-place in normal mode;
- remove incomplete outputs on failure;
- clean workspace after completion/cancel;
- document that deletion from SSD/APFS is not secure erase.

## Tests first

- [ ] source checksum unchanged;
- [ ] staging bytes equal source bytes before cleaning;
- [ ] failure before commit leaves no final output;
- [ ] valid commit creates final output;
- [ ] collision handling prevents overwrite;
- [ ] cancellation removes incomplete staged output;
- [ ] already committed earlier batch outputs survive later cancellation.

- [ ] Implement.

- [ ] Commit.

---

# Task 9 — Implement inspection parser and before/after diff model

## Goal

Present MAT2's own detected metadata semantically without creating a wrapper-defined risk model.

## Files

```text
app/src-tauri/src/model.rs
app/src-tauri/src/jobs.rs
app/src/types.ts
```

## Required result model

Conceptually:

```text
MetadataEntry {
  key
  display_value
}

MetadataDiff {
  key
  before
  after
  status: Removed | Changed | Remaining
}
```

If MAT2's human CLI output is unsuitable for reliable structured parsing, prefer a minimal local adapter using MAT2's own Python/libmat2 API **without changing sanitisation semantics**. Document the choice.

Do not use ExifTool directly merely for the comparison.

## Tests

- [ ] removed field;
- [ ] changed field;
- [ ] remaining field;
- [ ] empty before;
- [ ] empty after;
- [ ] duplicate/complex upstream keys;
- [ ] values containing HTML/control characters are data.

- [ ] Implement.

- [ ] Commit.

---

# Task 10 — Implement the job state machine

## Required states

```text
Selected
Queued
Inspecting
Processing
Verifying
Success
Warning
Failed
Cancelled
Unsupported
```

## Transition rules

Normal success path:

```text
Queued
→ Inspecting
→ Processing
→ Verifying
→ Success or Warning
```

Green Success requires:

1. cleaning process completed acceptably;
2. expected output exists;
3. output is a regular file;
4. output is non-empty;
5. post-inspection completed;
6. final output was committed.

Warning may represent a processed output where MAT2 still detects metadata or reports a meaningful limitation.

Failure covers missing/unverifiable/invalid output.

## Tests first

- [ ] child exit success + no output => Failed;
- [ ] output exists + verification fails => Failed or explicit verification failure, never Success;
- [ ] output verifies with remaining metadata => Warning;
- [ ] full successful path => Success;
- [ ] cancellation path => Cancelled;
- [ ] unsupported input => Unsupported;
- [ ] deleted-after-selection => Failed;
- [ ] permission error => Failed.

- [ ] Implement sequential batch execution.

No parallel processing in v1.

- [ ] Commit.

---

# Task 11 — Implement cancellation

## Behavior

When user cancels:

1. do not begin the next queued file;
2. terminate the current MAT2 child process in a controlled platform-appropriate way;
3. mark current item Cancelled;
4. delete incomplete staged artifacts;
5. retain already verified/committed outputs;
6. clean job workspace.

## Tests

- [ ] cancel before first file;
- [ ] cancel during current file;
- [ ] cancel after previous files committed;
- [ ] no later files start.

- [ ] Implement.

- [ ] Commit.

---

# Task 12 — Build the monolithic interface

## Files

```text
app/index.html
app/src/main.ts
app/src/state.ts
app/src/types.ts
app/src/styles.css
```

Implement exactly the one-window model from `INTERFACE.md`.

## Required sections

Left:

- drag/drop zone;
- Choose Files;
- Choose Folder;
- file rows with checkboxes;
- extension-group toggles;
- cleaning mode;
- output mode;
- Advanced disclosure;
- Process/Cancel.

Right:

- selected-file metadata before processing;
- after processing: before/after comparison;
- technical process details/log beneath;
- result summary.

Footer:

- MAT2;
- Dangerzone;
- PrivacyTools;
- “No telemetry” text is acceptable if true.

## Tests/checks

- [ ] no `innerHTML` for untrusted data;
- [ ] keyboard focus visible;
- [ ] status not encoded only by color;
- [ ] resizing to minimum remains usable;
- [ ] no remote assets;
- [ ] no preview rendering.

- [ ] Commit.

---

# Task 13 — Implement complete upstream option parity

## Goal

Every CLI option in the supplied MAT2 version is consciously represented.

Create:

```text
docs/MAT2_OPTION_PARITY.md
```

Table:

```text
Upstream option | UI location | Wrapper behavior | Tests
```

Examples of possible categories:

- processing option;
- Advanced option;
- diagnostic action;
- implicit/internal;
- deprecated/non-operative and intentionally not exposed.

Do not leave an upstream option unreviewed.

### In-place rule

If current upstream supports in-place:

- off by default;
- not persisted;
- explicit warning on activation;
- second confirmation on job start;
- disables normal output routing;
- verifies the modified source afterward.

Test stale frontend state cannot silently re-enable it on restart/new session.

- [ ] Commit.

---

# Task 14 — Implement hard-coded external links and Reveal Output

## File

```text
app/src-tauri/src/external.rs
```

## External functions

Prefer dedicated semantic calls:

```text
open_mat2_site()
open_dangerzone_site()
open_privacytools_site()
reveal_output(job_id)
```

URLs are constants in Rust.

Do not expose `open_url(url)`.

Reveal resolves only output paths owned by completed/current job state.

Do not expose arbitrary `open_path(path)`.

Configure opener capability scopes as narrowly as current Tauri permits.

## Tests

- [ ] only three URL destinations exist;
- [ ] query params are absent;
- [ ] arbitrary URL from frontend is impossible;
- [ ] unknown job ID cannot reveal arbitrary path.

- [ ] Commit.

---

# Task 15 — Add startup/runtime diagnostics

At startup/session initialization:

- locate packaged/development MAT2 runtime;
- query actual version;
- run dependency check;
- list formats or validate basic readiness as appropriate.

If fatal diagnostics fail:

- keep UI open;
- disable Process;
- show clear technical reason;
- do not attempt “best effort” cleaning.

Expose diagnostics in Advanced:

```text
MAT2 version
Supported formats
Check dependencies
MAT2 help
```

## Tests

- [ ] MAT2 missing;
- [ ] wrong configured executable;
- [ ] dependency check failure;
- [ ] healthy runtime;
- [ ] diagnostic output safely rendered.

- [ ] Commit.

---

# Task 16 — Security regression suite

Add targeted automated tests for:

## Argument injection

Names resembling:

```text
--version.jpg
--help.png
"; touch owned".jpg
$(touch owned).jpg
```

Expected: treated as paths/data.

## HTML/DOM injection

```text
<img src=x onerror=alert(1)>.jpg
<script>...</script>.pdf
```

Expected: literal text only.

## Terminal/control injection

Filenames/process messages with ANSI, ESC, CR tricks.

Expected: sanitized plain text.

## Path escape

Crafted relative paths cannot write outside approved root.

## Symlink

Selected/enumerated symlinks do not cause unexpected target processing or traversal.

## IPC

Audit registered commands and ensure there is no generic:

```text
exec
shell
read arbitrary path
write arbitrary path
open arbitrary URL
open arbitrary path
```

- [ ] All regression tests pass.

- [ ] Commit.

---

# Task 17 — Integration fixtures and real MAT2 tests

Use MAT2 against actual representative files.

Minimum where supported by packaged dependencies:

```text
JPEG
PNG
PDF
DOCX
ZIP/archive
MP3/audio
MP4/video
```

For each:

```text
fixture
→ pre-inspect
→ clean
→ verify output exists
→ post-inspect
→ wrapper result
```

If a dependency is optional in a developer environment, a test may skip with an explicit reason.

A public release claiming support for that format may not silently omit its required runtime dependency.

- [ ] Commit.

---

# Task 18 — Manual QA protocol

Create:

```text
docs/TEST_PROTOCOL.md
```

## Test A — public image with metadata

Use a known public metadata sample, e.g. from:

```text
https://github.com/ianare/exif-samples
```

The app itself does not download it.

Record:

1. source URL;
2. SHA-256;
3. MAT2 pre-inspection;
4. optional observational ExifTool output for QA only;
5. wrapper processing;
6. wrapper before/after comparison;
7. wrapper verification;
8. manual MAT2 post-inspection;
9. optional observational ExifTool post-inspection;
10. original SHA-256 after test to prove unchanged.

ExifTool in QA is an observer, not a product sanitizer.

## Test B — PDF with metadata

Verify:

- source remains intact in normal mode;
- output opens;
- MAT2 process completes;
- post-inspection completes;
- UI does not overclaim.

## Test C — folder batch

Create:

```text
batch/
  a.jpg
  b.jpg
  c.png
  d.pdf
  ignore.txt
```

Test:

- extension group toggles;
- individual uncheck;
- output layout;
- sequential statuses.

## Test D — cancellation

Cancel a multi-file job and confirm already committed outputs survive while incomplete work does not.

- [ ] Record results.

- [ ] Commit.

---

# Task 19 — Package MAT2 for macOS

This is a release-engineering task, not a license to change sanitisation.

Create:

```text
scripts/package-mat2-macos.sh
```

The final public app should not require an ordinary non-technical user to manually construct Homebrew/Python dependencies if a safe self-contained package can be produced.

The packaged runtime must include the dependencies needed for the formats the release claims to support.

At implementation time, derive the exact dependency set from the supplied MAT2 version.

## Packaging checks

- [ ] MAT2 source snapshot recorded;
- [ ] Python/runtime version pinned;
- [ ] dynamic libraries inspected;
- [ ] runtime works from inside application bundle;
- [ ] `--version` works;
- [ ] `--check-dependencies` passes for advertised support;
- [ ] smoke-clean fixture works;
- [ ] package manifest/hashes generated;
- [ ] third-party licenses included.

Do not download runtime components on first launch.

Do not silently call a random global `mat2` in a public release.

### Release gate

If a trustworthy relocatable runtime cannot be produced, do not label the public release complete. Keep a clearly documented developer build rather than weakening the architecture.

- [ ] Commit.

---

# Task 20 — Dependency and supply-chain review

Create/update:

```text
docs/THIRD_PARTY_LICENSES.md
docs/SECURITY.md
```

Run appropriate current tooling:

```text
cargo test
cargo fmt --check
cargo clippy
cargo audit
cargo deny
npm audit
```

Also run upstream MAT2 test suite in the packaged/development environment.

Review every added npm and Rust dependency.

For each nontrivial dependency, answer:

```text
Why is this needed?
Can standard library/native Tauri functionality replace it?
Does it add network, parser or process-execution surface?
```

Remove unjustified dependencies.

- [ ] Commit.

---

# Task 21 — Network audit

Prove the app itself does not require network access.

Check:

- source for `fetch`;
- XMLHttpRequest;
- WebSocket;
- HTTP client crates;
- remote CSS/fonts/images/scripts;
- analytics;
- updater;
- remote crash reporting;
- remote configuration.

Test the release candidate while the machine is offline.

External footer links are allowed only after explicit user click and open in the default system browser.

Record result in:

```text
docs/SECURITY_REVIEW_V1.md
```

- [ ] Commit.

---

# Task 22 — Capability / process / filesystem audit

Create a release-review table.

## Tauri capabilities

For every granted capability:

```text
Permission
Reason
Frontend surface using it
Can it be narrowed?
```

Anything without a clear reason is removed.

## Process creation

Search all wrapper source for:

```text
Command::new
spawn
kill
shell
exec
```

Document every call site.

Expected: process creation is centralized around MAT2/runtime management, not arbitrary frontend input.

## Filesystem mutation

Search:

```text
write
copy
rename
remove
create_dir
```

Verify mutation is limited to:

- private staging;
- approved output roots;
- explicit in-place upstream mode when user confirmed.

- [ ] Commit audit.

---

# Task 23 — Final security review

Create:

```text
docs/SECURITY_REVIEW_V1.md
```

For each control, mark:

```text
PASS
FAIL
NOT APPLICABLE
```

At minimum:

- upstream snapshot integrity;
- upstream tests;
- no modified MAT2 sanitisation;
- strict CSP;
- minimal capabilities;
- no generic shell;
- no generic filesystem IPC;
- no generic URL opener;
- no document previews;
- hostile filename tests;
- symlink/path confinement tests;
- false-success tests;
- output collision tests;
- destructive-mode confirmation tests;
- no telemetry;
- offline operation;
- runtime dependency diagnostics;
- post-clean MAT2 verification;
- original unchanged in normal mode;
- third-party license inclusion.

Any FAIL blocks release unless explicitly accepted by the owner as a documented residual risk.

---

# Task 24 — Release candidate

Build macOS release candidate.

Test:

- [ ] from `/Applications`;
- [ ] offline;
- [ ] fresh user account;
- [ ] machine/environment without unrelated global MAT2 being relied upon;
- [ ] Unicode filenames;
- [ ] leading-hyphen filenames;
- [ ] read-only output failure;
- [ ] 50+ file batch;
- [ ] cancellation;
- [ ] custom output;
- [ ] same-source output;
- [ ] inspect only;
- [ ] normal clean;
- [ ] lightweight;
- [ ] every actual upstream Advanced option;
- [ ] MAT2/Dangerzone/PrivacyTools links;
- [ ] Reveal Output;
- [ ] app restart resets destructive option.

Sign and notarize macOS distribution when public distribution is intended.

---

# Definition of Done

The implementation is done only when:

1. a non-technical user can open the app;
2. select files or a folder;
3. filter/uncheck a batch;
4. inspect MAT2-detectable metadata;
5. choose normal or lightweight behavior;
6. process sequentially;
7. see a before/after comparison;
8. see technical process details;
9. receive clear Warning/Failure rather than false success;
10. reveal cleaned outputs;
11. operate fully offline;
12. normal mode leaves originals byte-identical;
13. MAT2 is still the sanitisation authority;
14. the wrapper has no generic shell/arbitrary file/arbitrary URL surface exposed to the WebView;
15. the security review contains no unaccepted release-blocking failures.

---

# Explicit non-goals for this plan

Do not implement during v1 execution:

- metadata spoofing;
- synthetic personas;
- “smart spoof”;
- risk score;
- thumbnails;
- document preview;
- watch folders;
- background daemon;
- OCR;
- redaction;
- alternate metadata sanitizer;
- AI;
- cloud;
- telemetry;
- account;
- updater;
- plugin marketplace;
- Windows release claims without runtime validation.

If one of these appears necessary while implementing, stop and ask the owner rather than silently expanding scope.
