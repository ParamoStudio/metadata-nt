# Porting metadata'nt to Linux and Windows

metadata'nt is a Tauri 2 desktop wrapper around the MAT2 metadata cleaner
(Python). v1 ships macOS-only. This document is the complete technical brief
for porting it to Linux and Windows: a short chronological checklist for
humans, and a self-contained master prompt for AI agents.

Reference state: tag `v0.1.0`, branch `main`,
`https://github.com/ParamoStudio/metadata-nt`.

Architecture in one paragraph: the WebView frontend (`app/`, vanilla TS +
Vite, zero networking by design) talks to a Rust core (`app/src-tauri/src/`)
through a 30-command typed IPC allow-list. Rust owns all process creation
(MAT2 runtime), all networking (two audited paths: Canarytokens tripwire,
GitHub update checker) and all URL opening (compile-time allow-list). The
MAT2 runtime ships as a frozen PyInstaller onedir bundle at
`app/src-tauri/resources/mat2-runtime/` (gitignored; reproduced by
`scripts/package-mat2-macos.sh` from the vendored upstream snapshot in
`upstream-mat2/`, gitignored, integrity-checked by `scripts/verify-upstream.sh`).

Upstream caveat: MAT2 0.15.0 officially supports Linux; macOS support is
Homebrew-based; Windows is **not** officially supported upstream (WSL is the
documented route). A native Windows port is therefore best-effort: every
format parser must pass the verification battery on Windows before claiming
parity. Linux is the natural first port.

---

## Humans — chronological checklist

### Both platforms

1. Install toolchain: Rust stable ≥ 1.90 (rustup), Node ≥ 20.19, Python 3.14,
   git. Clone: `git clone https://github.com/ParamoStudio/metadata-nt.git &&
   cd metadata-nt && git checkout v0.1.0`.
2. Fetch and verify the vendored MAT2 snapshot: `scripts/verify-upstream.sh`
   (creates `upstream-mat2/` at the pinned commit; see
   `docs/UPSTREAM_SNAPSHOT.md`).
3. Read `docs/SECURITY.md` and `HANDOFF.md` §18 before changing anything: the
   security invariants below are load-bearing, not style.

### Linux

4. Install Tauri v2 system deps (Debian/Ubuntu names): `libwebkit2gtk-4.1-dev
   build-essential curl wget file libxdo-dev libssl-dev
   libayatana-appindicator3-dev librsvg2-dev` plus GTK3 dev packages.
5. Install MAT2 runtime deps system-wide or in a venv: poppler-glib, librsvg,
   ffmpeg, exiftool, python3-gi, pycairo, mutagen (see the `BREW_VERSIONS`
   block of `scripts/package-mat2-macos.sh` for the macOS equivalent set and
   `docs/THIRD_PARTY_LICENSES.md` for provenance).
6. Port the opener: `external.rs` → `platform_open` is
   `#[cfg(target_os = "macos")]` (uses `open`). Add a `#[cfg(target_os =
   "linux")]` branch using `xdg-open` with an argv vector (never a shell).
   Keep the allow-list invariant: no URL arrives from the frontend except the
   validated release tag path.
7. Process cancellation already works: `jobs.rs` uses Unix process groups
   (`libc`, `cfg(unix)`) — no change needed on Linux.
8. Write `scripts/package-mat2-linux.sh` mirroring
   `scripts/package-mat2-macos.sh` stage by stage (venv + PyInstaller onedir,
   staged upstream prune, clean-env verification battery, MANIFEST.sha256).
   Keep the `__pycache__` pruning (release hygiene).
9. Add Linux bundle targets in `app/src-tauri/tauri.conf.json`
   (`"bundle" → "targets"`: `deb`, `appimage`) — keep macOS targets.
10. Verify: `cd app/src-tauri && cargo test` (integration tests use the dev
    fallback `.venv/bin/python` + `upstream-mat2/mat2`), `cd app && npm run
    lint:all && npm run build`, then `scripts/package-mat2-linux.sh` and a
    release build via the `scripts/build-release.sh` pattern (extend the
    `--remap-path-prefix` flags to the Linux build root so binaries carry no
    build-host paths).
11. Run the manual QA protocol `docs/TEST_PROTOCOL.md` (Tests A–I) on the
    Linux build before calling it release-quality.

### Windows

12. Install MSVC Build Tools, the `x86_64-pc-windows-msvc` Rust target, and
    ensure WebView2 runtime (preinstalled on Windows 10/11). Tauri bundling:
    add `nsis` (and optionally `msi`) to `bundle.targets`.
13. Port the opener: `external.rs` → add a `#[cfg(target_os = "windows")]`
    branch. Prefer `explorer.exe <url>` as an argv vector or
    `ShellExecuteW` via `windows-sys`; never `cmd /c start` (shell
    interpolation violates the process-creation policy).
14. Port process cancellation: `jobs.rs` kills the MAT2 process group with
    `libc kill(-pgid, …)` (`cfg(unix)` only). On Windows use a Job Object
    (`CreateJobObjectW` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` +
    `AssignProcessToJobObject`) via `windows-sys`. MAT2 spawns
    ProcessPoolExecutor workers; killing only the parent orphans them.
    Adding `windows-sys` is a new dependency: update the supply-chain table
    in `docs/SECURITY.md` and the cargo-deny allow-list in the same commit.
15. Port the dev/runtime resolution paths in `mat2_runner.rs`: the dev
    fallback assumes `.venv/bin/python` and the shebang script
    `upstream-mat2/mat2`; on Windows use `.venv\Scripts\python.exe` and invoke
    the CLI as `python.exe <root>\upstream-mat2\mat2`.
16. Write `scripts/package-mat2-windows.ps1` (or bash under Git Bash): Python
    embeddable or installed Python 3.14, pip mutagen + pyinstaller, exiftool
    Windows build, ffmpeg Windows build; MAT2's poppler/librsvg-dependent
    parsers (PDF, SVG, ePub images) are the risky area — test each format
    explicitly. Note: the lint gates (`scripts/check-*.sh`) are bash; run
    them under Git Bash in CI.
17. Verify with the same battery as step 10 (cargo test, lint:all, build,
    packaging verification, QA protocol A–I on a real Windows machine).
    Until that QA passes, label Windows builds **experimental** — do not
    publish them as release-quality in a security product.

### Rules that apply to every port

- No new IPC command without updating `scripts/check-ipc-surface.sh`
  allow-list AND `docs/SECURITY.md` in the same commit.
- No frontend networking, ever (`scripts/check-no-html-sinks.sh` enforces).
- No new remote URL in app source without extending
  `scripts/check-no-remote-refs.sh` allow-list deliberately.
- No shell interpolation anywhere in process creation; argv vectors only.
- Keep release binaries free of build-host paths
  (`scripts/build-release.sh` pattern).
- Small, isolated commits; no secrets, no machine paths, no personal data in
  any commit or artifact.

---

## Agents — master prompt

You are a systems engineer porting the metadata'nt desktop application
(Tauri 2 + Rust + vanilla TS frontend + frozen Python MAT2 runtime) from
macOS to TARGET_OS (linux or windows). Work only in a fresh clone:
`git clone https://github.com/ParamoStudio/metadata-nt.git && cd metadata-nt
&& git checkout -b port/TARGET_OS main`. Read `PORTING.md` (this file),
`docs/SECURITY.md`, `HANDOFF.md` §18 and `docs/TEST_PROTOCOL.md` first.

Goal: a compiling, tested, packageable TARGET_OS build with identical
security properties to the macOS v0.1.0 release. Do not redesign the app, do
not add frameworks, do not touch MAT2 sanitisation logic, do not add
networking, do not weaken any lint gate.

Exact porting surface (search these symbols, they are the complete set of
OS-specific code):
1. `app/src-tauri/src/external.rs` — `platform_open` (macOS-only today).
   Add the TARGET_OS branch; argv vectors only, no shell. Preserve
   `open_approved_url` allow-list semantics and `open_release_page` prefix
   guard.
2. `app/src-tauri/src/jobs.rs` — process-group cancellation (`libc`,
   `cfg(unix)`). Linux: unchanged. Windows: Job Objects via `windows-sys`
   (new dependency → update `docs/SECURITY.md` supply-chain table in the
   same commit).
3. `app/src-tauri/src/mat2_runner.rs` — `dev_manifest_dir()` and the dev
   fallback paths (`.venv/bin/python`, `upstream-mat2/mat2`); adapt to
   TARGET_OS layouts (Windows: `.venv\Scripts\python.exe`,
   `python.exe …\upstream-mat2\mat2`).
4. `scripts/package-mat2-macos.sh` — the reference packaging pipeline
   (stages: upstream integrity → venv+PyInstaller → staged prune → freeze →
   clean-env verification battery → install + MANIFEST.sha256). Write the
   TARGET_OS equivalent with the same stages and the same verification
   battery; keep `__pycache__` pruning.
5. `app/src-tauri/tauri.conf.json` — extend `bundle.targets` for TARGET_OS
   (linux: deb+appimage; windows: nsis), keep existing targets.
6. `scripts/build-release.sh` — extend the path-remap flags so TARGET_OS
   release binaries embed no build-host paths.

Setup before coding: `scripts/verify-upstream.sh`; install TARGET_OS
dependencies (linux: webkit2gtk-4.1/gtk3/librsvg2 dev packages + poppler-glib
+ ffmpeg + exiftool + python3-gi + pycairo + mutagen; windows: MSVC, WebView2,
exiftool/ffmpeg Windows builds; MAT2 Windows support is unofficial — treat
parser parity as unproven until tested).

Verification loop after every change: `cd app/src-tauri && cargo test`;
`cd app && npm run lint:all && npm run build`; packaging script; manual QA
`docs/TEST_PROTOCOL.md` A–I on a real TARGET_OS machine (CI build alone is
NOT release-quality). All 115+ Rust tests must stay green; the three lint
gates must pass unmodified except deliberate allow-list extensions documented
in `docs/SECURITY.md`.

Definition of done: clean clone on TARGET_OS builds dev and release; runtime
packaging passes its verification battery; cargo test + lint:all green; QA
A–I passed and recorded; binaries carry no build-host paths; PR contains
small isolated commits, no secrets, no personal or machine metadata;
`docs/SECURITY.md` reflects every dependency/IPC/URL change. If any step is
blocked by upstream MAT2 limitations on TARGET_OS (especially Windows), stop
and report the exact blocker instead of weakening a gate or faking parity.
