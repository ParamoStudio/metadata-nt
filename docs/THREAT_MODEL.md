# MAT2 Wrapper — Threat Model (v1)

Derived from `HANDOFF.md` §6, §11, §12 and `IMPLEMENTATION_PLAN.md` "Review focus".
Scope: the wrapper only. Upstream MAT2 internals have their own threat model
(`upstream-mat2/doc/threat_model.md`); we do not modify or re-audit its cleaning logic.

## 1. Assets

| Asset | Sensitivity |
|---|---|
| User original files (selected inputs) | High — private content + metadata the user wants removed |
| Cleaned outputs | High — same content, must not be corrupted or silently unsanitised |
| Filenames and directory structure | Medium–High — can reveal identity/projects; must not be persisted in logs |
| MAT2-detected metadata (before/after values) | High — displayed in-memory only, never persisted |
| The MAT2 runtime itself | Integrity-critical — it is the sanitisation authority |

## 2. Trust boundaries

```
[untrusted] user files, filenames, file content, MAT2 stdout/stderr
      │
      ▼  (B1) selection & path handling — Rust validates, confines, never shell-expands
[Rust core] — trusted; the only component that touches paths and spawns processes
      ▲
      │  (B2) typed IPC — opaque IDs in, semantic commands only; no generic
      │      exec/shell/read/write/open surface (HANDOFF §11)
[WebView frontend] — treated as POTENTIALLY COMPROMISED (XSS via displayed
      │               strings is the classic vector; mitigated by textContent-only
      │               rendering + strict CSP + minimal capabilities)
      ▼  (B3) std::process::Command — argument vector, never a shell string;
[MAT2 CLI]   `--` end-of-options before file paths (verified supported, snapshot §4.4)
```

Capabilities/CSP limit the blast radius of a frontend compromise; they do not make
unsafe Rust code safe (HANDOFF §12). Every Rust command re-validates its inputs
against the registry of approved paths, regardless of what the frontend claims.

## 3. In scope — what v1 actively reduces (HANDOFF §6.1)

The five release-critical failure classes (IMPLEMENTATION_PLAN "Review focus"):

1. **Hostile filename / argument injection** — names starting with `-`, quotes,
   shell syntax, Unicode, control characters.
   Mitigations: no shell ever; argv vectors only; `--` before paths; Rust-side
   path validation; log sanitisation strips control chars (Task 5, 6, 16).
2. **Path escape / symlink behavior** — `..`, symlinked components, crafted
   relative paths in enumeration and output commit.
   Mitigations: enumeration without following symlinked dirs; canonicalised
   output roots with containment checks; collisions resolved without overwrite
   (Task 4, 7, 8, 16).
3. **False success** — exit code alone never yields green.
   Mitigations: success requires output exists + regular file + non-empty +
   post-inspection completed + committed (Task 8, 9, 10). `--show` always exits 0
   upstream, so support/failure detection parses output, never just exit codes
   (snapshot §4.5–4.6).
4. **Frontend compromise blast radius** — WebView has no generic shell,
   filesystem or URL-opening power.
   Mitigations: explicit single capability file with `core:event:default` only;
   strict CSP (no inline script, no remote sources, frame/object 'none');
   three hard-coded link openers implemented as dedicated Rust commands
   (Task 3, 12, 14, 22).
5. **Accidental network / supply-chain behavior** — app is offline by design.
   Mitigations: no fetch/XHR/WebSocket in app source (enforced by
   `scripts/check-no-remote-refs.sh`); no telemetry/updater/remote assets;
   pinned upstream snapshot with GPG-verified tag + `verify-upstream.sh`;
   dependency review (Task 19–21).

Also in scope (HANDOFF §6.1): no output collision/overwrite; originals never
modified in normal mode; destructive in-place mode off every launch behind double
confirmation; no stale UI state re-enabling destructive mode; no persistent logs
containing filenames; no parsing of hostile documents by the GUI (no previews).

## 4. Out of scope — explicitly NOT protected (HANDOFF §6.2)

- Compromised OS / firmware; malware in the user account; state actor with local code execution
- Unknown MAT2 bugs; metadata MAT2 cannot detect (UI must say "No metadata
  detectable by MAT2", never "metadata-free")
- Identifying visible content inside files; PRNU/sensor fingerprinting;
  steganographic or adversarial watermarks
- OS caches, backups, snapshots, indexing, forensic traces (staging deletion on
  SSD/APFS is NOT secure erase — documented in Task 8)
- Supply-chain compromise of the development machine
- Hostile-document exploitation inside upstream parsers beyond what invoking
  MAT2 directly would expose (upstream removed bubblewrap sandboxing in 0.14.0;
  on macOS there is no equivalent wrapper-side sandbox in v1 — residual risk,
  accepted: the wrapper adds no new parser surface of its own)

## 5. Marketing constraints

Never described as "anonymous", "untraceable", "100% clean" or "state-actor
proof". The epistemic limit of `--show` is preserved in UI copy.
