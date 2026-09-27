# MAT2 Tauri Wrapper — Project Handoff

## 0. Purpose of this document

This document is the project-context handoff for an implementation agent.

You are being given:

1. this handoff;
2. `INTERFACE.md`;
3. `IMPLEMENTATION_PLAN.md`;
4. a local copy of the current upstream MAT2 repository, supplied by the project owner.

Read all three documents before writing product code.

The local MAT2 repository supplied by the owner is the authoritative implementation input. Do not replace it with a different fork, silently update it, or modify upstream cleaning logic.

The product is **not a new metadata-cleaning engine**. It is a small, security-conscious desktop wrapper around MAT2.

---

## 1. Product in one sentence

Build a small, dark, offline Tauri desktop application that lets non-technical users inspect, batch-select, clean and verify files using the supplied upstream MAT2 implementation, while adding as little new attack surface and policy as possible.

---

## 2. Why this project exists

MAT2 is a mature metadata-removal toolkit whose design is aligned with privacy/anonymisation workflows. It exposes a Python library and CLI and has historically been integrated into privacy-focused tooling.

The usability gap this project targets is narrower:

- MAT2 is accessible to technical users through a CLI.
- Linux users have had GUI integrations around MAT2.
- The project owner wants a straightforward desktop experience, with macOS first and a path to broader distribution.
- The wrapper should preserve MAT2 as the authority for sanitisation rather than reimplementing file-format cleaning.

The wrapper exists to make MAT2 clickable, understandable, batch-friendly and auditable.

---

## 3. Core trust decision

### Trusted sanitisation engine

The sanitisation authority is the supplied upstream MAT2 code.

The wrapper MUST NOT:

- parse EXIF itself;
- rewrite PDF metadata itself;
- alter OOXML metadata itself;
- invent a fallback metadata remover;
- call ExifTool directly as an alternate sanitiser;
- silently fall back to Pillow, pypdf, python-docx or another writer;
- implement synthetic/spoofed metadata in v1.

If MAT2 cannot process a file, the wrapper reports that fact.

**Fail closed rather than silently substituting another engine.**

### Wrapper responsibility

Our code is responsible only for:

- user selection;
- batch construction;
- safe path handling;
- typed option mapping;
- launching MAT2 without a shell;
- collecting output;
- staging/commit semantics;
- verification;
- presenting before/after information;
- reporting failures accurately.

---

## 4. Local upstream repository is authoritative

Before implementation, inspect the supplied MAT2 tree.

Run, where applicable:

```bash
git status --short
git remote -v
git rev-parse HEAD
git describe --tags --always --dirty
git log -1 --show-signature
```

Also inspect:

```text
README.md
INSTALL.md
CHANGELOG.md
CONTRIBUTING.md
pyproject.toml
mat2
libmat2/
tests/
doc/
```

Record the results in a new project file:

```text
docs/UPSTREAM_SNAPSHOT.md
```

It must include:

- upstream repository URL;
- supplied commit hash;
- nearest tag/version;
- dirty/clean state;
- signature information if available;
- Python requirement;
- runtime dependencies;
- exact CLI help output;
- exact list of supported formats as reported by the supplied version;
- date the snapshot was recorded.

### Important rule

The local copy supplied by the owner wins over assumptions in these documents if upstream changed after this handoff was written.

If a material difference is discovered:

1. stop;
2. document the difference;
3. adapt the wrapper mapping to the actual supplied upstream;
4. do not modify MAT2 merely to make this plan easier.

---

## 5. MAT2 characteristics to understand before coding

At the time this handoff was prepared, the official MAT2 repository described:

- a `libmat2` Python library;
- an eponymous `mat2` CLI;
- Python 3.11+;
- support via dependencies such as Mutagen, Poppler/Cairo, GdkPixbuf, librsvg, FFmpeg and ExifTool;
- macOS dependency installation guidance;
- normal cleaning that produces a new `.cleaned` file rather than changing the original;
- a lightweight mode;
- metadata inspection/show mode;
- archive unknown-member policies;
- dependency and format diagnostics.

Do **not** rely on this paragraph as API truth. Verify every option against the supplied source and `mat2 --help`.

MAT2 itself warns that a scan showing no detectable metadata is **not a mathematical guarantee that no metadata of any kind exists**. Preserve that epistemic limitation in the UI.

---

## 6. Threat model

### We protect against wrapper-level mistakes

The v1 design should actively reduce:

- shell/command injection;
- argument injection from filenames;
- HTML/DOM injection through displayed filenames or process output;
- path traversal;
- unsafe symlink following;
- output collision/overwrite;
- accidental modification of originals;
- stale UI state enabling destructive mode;
- false-success states;
- unsupported files being presented as sanitised;
- accidental remote resources or telemetry;
- overly broad Tauri permissions;
- unnecessary parsing of potentially hostile documents by the GUI;
- persistent application logs containing sensitive filenames.

### We do not claim to protect against

- an already compromised operating system;
- malware with access to the user account or files;
- compromised firmware;
- a state actor with arbitrary local code execution;
- unknown MAT2 bugs;
- unknown metadata MAT2 cannot detect;
- identifying content visible inside the file;
- camera sensor / PRNU fingerprinting;
- steganographic or adversarial watermarks;
- OS caches, backups, snapshots, indexing or forensic traces;
- supply-chain compromise of the development machine;
- hostile-document exploitation inside upstream parsers beyond what our wrapper can constrain.

Do not market the application as “anonymous”, “untraceable”, “100% clean” or “state-actor proof”.

---

## 7. Relationship to Dangerzone

Dangerzone solves a related but different problem.

Use the mental model:

```text
Potentially hostile document
        ↓
    Dangerzone
        ↓
safer reconstructed document

Metadata disclosure problem
        ↓
       MAT2
        ↓
sanitised copy + verification
```

Dangerzone is designed around hostile documents and sandboxing/conversion.

This MAT2 wrapper is designed around **metadata minimisation while preserving the file type/workflow where MAT2 supports it**.

Do not embed Dangerzone or invoke it in v1.

The UI may provide a plain external link explaining that users who do not trust a document should consider Dangerzone.

---

## 8. Relationship to ExifCleaner and DMS

### ExifCleaner

ExifCleaner is useful prior art for approachable desktop metadata cleaning, but its underlying model is centered on ExifTool.

Do not turn this project into an ExifCleaner clone.

### Deceptive Metadata Shredder (DMS)

Repository:

https://github.com/davvikq/deceptive-metadata-shredder

DMS is useful as **UX and defensive-engineering inspiration**, not as the trusted sanitisation engine for this project.

Useful ideas to reuse conceptually:

- inspect before processing;
- inspect after processing;
- visible before/after comparison;
- staged/atomic output handling;
- validation before declaring success;
- path-safety thinking;
- explicit status per file;
- warning when a file looks already processed.

Do not copy into v1:

- metadata spoofing;
- risk scoring;
- watch folders/daemon behavior;
- thumbnail or preview parsing;
- fallback document parsers;
- persistent processing logs;
- alternate metadata-writing engines.

---

## 9. Product principles

### 9.1 Offline by design

The application itself requires no network access.

No:

- telemetry;
- analytics;
- crash upload;
- update polling;
- CDN;
- remote font;
- remote JS;
- remote CSS;
- remote image;
- web API;
- account;
- cloud sync.

External links are explicit user actions opened in the system browser.

Opening an external website naturally causes the browser/site to observe an ordinary web visit. The application must add no tracking query parameters, IDs or referrer-like custom data.

### 9.2 No document preview

Do not render user documents in the WebView.

No:

- image thumbnails;
- PDF viewer;
- SVG rendering from selected files;
- video player;
- Office preview;
- HTML preview.

Use file-type labels/icons only.

### 9.3 One sanitisation authority

MAT2 decides how content is cleaned.

The wrapper decides only how the user asks MAT2 to do it and whether the produced result is accepted/presented.

### 9.4 Fail visibly

A failure is a first-class result.

Never convert:

- missing output;
- MAT2 non-zero status;
- verification error;
- missing dependency;
- unsupported type;
- path error

into a green success state.

### 9.5 Originals remain originals

Default behavior never uses in-place processing.

If upstream still exposes in-place processing and the project chooses to expose it in Advanced settings, it must be off on every launch and require explicit destructive confirmation.

### 9.6 Small codebase over feature breadth

Prefer:

- vanilla TypeScript;
- simple HTML/CSS;
- small Rust modules;
- native dialogs;
- typed commands.

Avoid large UI frameworks unless a concrete implementation blocker requires one and the owner approves the change.

---

## 10. Architecture

Target architecture:

```text
┌──────────────────────────────────────────────────────┐
│               TAURI SYSTEM WEBVIEW                  │
│                                                      │
│  Static local HTML/CSS/TypeScript                   │
│  File list, options, compare view, status, log      │
└────────────────────────┬─────────────────────────────┘
                         │ typed IPC only
                         ▼
┌──────────────────────────────────────────────────────┐
│                    RUST CORE                         │
│                                                      │
│ selection registry                                  │
│ job state                                            │
│ safe path/output handling                           │
│ MAT2 argument builder                               │
│ child-process lifecycle                             │
│ result/verification parsing                         │
└────────────────────────┬─────────────────────────────┘
                         │ std::process::Command
                         │ no shell
                         ▼
┌──────────────────────────────────────────────────────┐
│               SUPPLIED MAT2 UPSTREAM                 │
│                                                      │
│ CLI / libmat2 / upstream dependencies               │
└──────────────────────────────────────────────────────┘
```

The frontend must never receive a generic “execute command” capability.

---

## 11. Trust boundary rules

### Frontend may request semantic operations

Examples:

```text
select_files
select_folder
remove_selected_items
inspect_selected
start_clean_job
cancel_job
choose_output_root
run_mat2_diagnostics
reveal_output
open_mat2_site
open_dangerzone_site
open_privacytools_site
```

### Frontend must NOT receive generic operations

Do not expose:

```text
run(command, args)
shell(string)
read_file(path)
write_file(path, bytes)
delete_file(path)
open_url(url)
open_path(path)
```

Use opaque IDs for selected files where practical.

Rust owns the mapping from UI IDs to actual approved paths.

---

## 12. Tauri security posture

Use Tauri 2.

Review current official documentation at implementation time:

- https://tauri.app/security/
- https://tauri.app/security/capabilities/
- https://v2.tauri.app/security/csp/
- https://v2.tauri.app/reference/javascript/opener/

Apply:

- one window;
- local bundled content only;
- strict CSP;
- smallest capability set possible;
- no generic shell plugin;
- no remote capabilities;
- no iframe;
- no embedded browser navigation;
- explicit allow-listing of external links;
- Rust-side validation even if Tauri capability scopes exist.

Remember: Tauri capabilities reduce impact of frontend compromise, but do not make unsafe Rust code safe.

---

## 13. External links

The footer may expose three explicit resources:

### MAT2
https://github.com/jvoisin/mat2

### Dangerzone
https://github.com/freedomofpress/dangerzone

### PrivacyTools
https://www.privacytools.io/

Use hard-coded destinations.

No UTM parameters.
No installation ID.
No analytics parameters.
No generic URL opener exposed to the frontend.

---

## 14. UI concept

The detailed UI specification is in `INTERFACE.md`.

High-level idea:

- one compact dark window;
- left: files + batch controls + cleaning options;
- right: metadata/result view plus technical process log;
- advanced options collapsed;
- no extra navigation hierarchy;
- no onboarding screen;
- no dashboards;
- no preview pane.

A visible before/after comparison is a core feature, inspired partly by DMS, but values must come from MAT2 inspection rather than a wrapper-authored “risk” model.

---

## 15. Processing concept

Default flow:

```text
SELECT
  ↓
PRE-INSPECT (MAT2)
  ↓
STAGE BYTE-IDENTICAL INPUT
  ↓
CLEAN (MAT2)
  ↓
VALIDATE OUTPUT EXISTS / IS REGULAR / NON-EMPTY
  ↓
POST-INSPECT (MAT2)
  ↓
BUILD BEFORE/AFTER DIFF
  ↓
ATOMICALLY COMMIT TO FINAL OUTPUT
  ↓
SHOW RESULT + REVEAL OUTPUT
```

“Success” is not equivalent to child process exit alone.

A verified green result requires the wrapper to complete its full result checks.

Use wording such as:

> No metadata detectable by MAT2

Never:

> Metadata-free

---

## 16. Batch model

Users can:

- drag files;
- choose multiple files;
- choose a folder.

For a selected folder:

- enumerate regular files;
- do not follow symlinked directories;
- group by extension;
- allow toggling extension groups;
- allow unchecking individual files;
- pass the resulting explicit file list to the job.

Extension filtering is a UI convenience, not a declaration of MAT2 support.

MAT2 remains the authority.

---

## 17. Output model

Default:

For each source parent directory:

```text
<source-parent>/
  MAT2 Output/
    YYYY-MM-DD_HHmmss/
      ...
```

For folder batches, preserve meaningful relative subdirectory structure where doing so does not create path-escape risk.

Offer a Custom Output Root setting.

Do not write automatic manifests or processing logs into output folders in v1.

Handle collisions without overwriting:

```text
photo.cleaned.jpg
photo.cleaned-2.jpg
photo.cleaned-3.jpg
```

---

## 18. Logging

The technical log is:

- in memory;
- visible in the app;
- plain text;
- not an interactive shell;
- not persisted by default.

Sanitise control characters before display.

Render with text APIs (`textContent` or equivalent), never `innerHTML`.

Do not create clickable URLs from process output.

---

## 19. Security audit philosophy

The wrapper cannot meaningfully defend against an operating system already controlled by a powerful attacker.

The audit therefore asks a narrower, useful question:

> Did our wrapper introduce a new way for untrusted filenames, frontend state, output paths, WebView content or process handling to violate the safety properties of invoking MAT2 directly?

Audit:

- process construction;
- Tauri capabilities;
- CSP;
- path handling;
- symlinks;
- output commit;
- frontend injection;
- dependency tree;
- network behavior;
- failure-state accuracy;
- destructive-mode safety;
- packaging integrity.

The implementation plan defines the release gate.

---

## 20. Distribution philosophy

Development may initially use the supplied source/runtime and locally installed dependencies.

A public application build should ultimately be self-contained enough that a non-technical user does not need to manually construct the MAT2 dependency environment.

However:

**do not compromise or fork MAT2 merely to make packaging convenient.**

Packaging the runtime is a separate engineering task and may be the hardest part of v1.

If a trustworthy self-contained build cannot yet be produced, ship no misleading “finished” release. A developer build that requires explicit dependencies is preferable to a silently incomplete sanitizer.

---

## 21. References the implementation agent should review

### MAT2
- https://github.com/jvoisin/mat2
- https://github.com/jvoisin/mat2/blob/main/README.md
- https://github.com/jvoisin/mat2/blob/main/INSTALL.md
- https://github.com/jvoisin/mat2/blob/main/CHANGELOG.md
- https://github.com/jvoisin/mat2/blob/main/doc/mat2.1
- supplied local `mat2` entrypoint
- supplied local `libmat2/`
- supplied local `tests/`

### Tauri
- https://tauri.app/security/
- https://tauri.app/security/capabilities/
- https://v2.tauri.app/security/csp/
- https://v2.tauri.app/reference/javascript/opener/

### Dangerzone
- https://github.com/freedomofpress/dangerzone
- https://github.com/freedomofpress/dangerzone/blob/main/SECURITY.md

### Prior-art UX / hardening reference
- https://github.com/davvikq/deceptive-metadata-shredder

### Privacy resource
- https://www.privacytools.io/

### Optional test corpus
- https://github.com/ianare/exif-samples

---

## 22. Non-goals for v1

Do not add:

- AI;
- metadata spoofing;
- smart spoof profiles;
- risk scoring;
- watch folders;
- background daemon;
- previews/thumbnails;
- OCR;
- redaction;
- EXIF editor;
- alternate sanitizer;
- history/database;
- telemetry;
- cloud;
- user accounts;
- updater;
- plugin system;
- multi-window UI;
- automatic Dangerzone execution;
- “secure delete” claims;
- Windows support until the MAT2 runtime is demonstrated and tested there.

---

## 23. Definition of product integrity

At the end of implementation, a reviewer should be able to say:

> The application is a narrow local desktop interface around the supplied MAT2 source. It does not implement its own metadata-cleaning policy, does not silently substitute alternate parsers, does not require network access, does not render user documents, uses a constrained Tauri boundary, verifies outputs before presenting success, and makes failure visible.

If that statement is no longer true, stop and reassess the architecture before continuing.
