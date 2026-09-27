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

Registry policy (selection.rs, covered by unit tests): frontend-supplied data is
only ever opaque IDs; unknown IDs resolve to nothing; duplicates dedupe by
canonical path; folder enumeration never follows symlinks; symlinked files are
classified explicitly; broken symlinks are skipped with a reason.

Planned additions (each lands with its rationale line here in the same commit):

- `opener:allow-open-url` scoped to the three exact URLs, OR (preferred) plain
  Rust-side opening with no frontend permission at all (Task 14 decides; default
  is Rust-side so no capability is added).
- `opener:allow-reveal-item-in-dir` or Rust-side equivalent for Reveal Output
  (Task 14).

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

## Known residual risks (accepted for v1)

1. No OS-level sandbox around MAT2 child processes on macOS (upstream removed
   bubblewrap in 0.14.0; it was Linux-only anyway). Exposure equals invoking
   MAT2 directly from a terminal — the wrapper adds no new parser surface.
2. Staging/workspace deletion is not secure erase on SSD/APFS (documented in UI
   docs and Task 8).
3. MAT2 `--show` output parsing depends on upstream output shape; a format
   change would degrade the diff view (fails visible, never false-success).
