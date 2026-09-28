# Third-Party Components & Licenses

Recorded for Task 20 (dependency & supply-chain review). Tooling results at the
bottom. The frozen runtime bundle ships the components in §3 inside
`MAT2 Wrapper.app/Contents/Resources/mat2-runtime/`; every file is enumerated
with its SHA-256 in the build manifest (`build/runtime-dist/MANIFEST.sha256`),
and exact component versions are pinned in `PACKAGE_INFO.json`.

## 1. Rust crates (app/src-tauri)

Validated by `cargo deny check` (advisories/bans/licenses/sources **ok**) with
the graph scoped to the shipping target `aarch64-apple-darwin`. License
allow-list: MIT, Apache-2.0, BSD-3-Clause, Zlib, Unicode-3.0, MPL-2.0,
LicenseRef-Proprietary (first-party). 432 crates in `Cargo.lock`.

Direct dependencies and justification (plan Task 20 questions):

| Crate | License | Why needed | Stdlib/Tauri replacement? | Added surface |
|---|---|---|---|---|
| `tauri` 2.12 | MIT OR Apache-2.0 | Application framework (the product) | — | WebView IPC boundary (constrained by capabilities/CSP) |
| `tauri-build` 2.7 | MIT OR Apache-2.0 | Build-time codegen | — | build-time only |
| `tauri-plugin-dialog` 2.8 | MIT OR Apache-2.0 | Native file/folder pickers, Rust-side only | Raw NSOpenPanel FFI = more unsafe code | None for frontend (no `dialog:*` permission granted); note: pulls `tauri-plugin-fs` transitively — **no fs permissions are granted** (Task 22 audit) |
| `serde` + `serde_json` 1.x | MIT OR Apache-2.0 | Typed IPC/DTO serialization + engine JSON protocol | std has no JSON | Parser limited to engine responses and typed frontend DTOs |
| `uuid` 1.x (v4) | Apache-2.0 OR MIT | Opaque selection ids, job ids, CSPRNG job seeds | std has no CSPRNG ids | none (getrandom-backed) |
| `sha2` 0.10 | MIT OR Apache-2.0 | Checksum invariants (source-unchanged, staging byte-identity, pack pin) | std has no hashing | none (pure Rust, RustCrypto) |
| `time` 0.3 | MIT OR Apache-2.0 | Local `YYYY-MM-DD_HHmmss` output timestamps | hand-rolled civil calendar = bug surface | none (no network/parser) |
| `libc` 0.2 (unix) | MIT OR Apache-2.0 | Process-group signalling (`kill(-pgid, …)`) for controlled cancellation of MAT2 children | std cannot signal groups | already an indirect dependency of the Rust std/tauri tree |
| `ureq` 2.x (+ rustls, webpki) | MIT OR Apache-2.0 (rustls: MIT OR Apache-2.0 OR ISC) | Investigation Tripwire HTTPS client — blocking, rustls TLS, no async runtime, no cookies; hard-coded Canarytokens.org origin | std has no HTTP/TLS stack | Rust-side only; no HTTP IPC command; WebView keeps zero network capability; cargo-deny license allow-list validated |

## 2. npm packages (app/)

`npm audit`: **0 vulnerabilities**. Build-time tooling is not shipped in the
binary (only its `dist/` output is bundled).

| Package | License | Role |
|---|---|---|
| `@tauri-apps/api` 2.12 | MIT OR Apache-2.0 | Typed IPC client (runtime, bundled) |
| `@tauri-apps/cli` 2.12 | MIT OR Apache-2.0 | dev/build |
| `typescript` 5.9 | Apache-2.0 | dev/build |
| `vite` 7.3 (+ esbuild MIT, rollup MIT, postcss MIT, …) | MIT | dev/build |

## 3. Frozen MAT2 runtime bundle (shipped inside the .app)

| Component | Version (pinned) | License | Notes |
|---|---|---|---|
| MAT2 (upstream source, verbatim, pruned of tests/CI) | 0.15.0 @ `70c17d3` | **LGPL-3.0-or-later** | LICENSE bundled at `_internal/upstream/LICENSE`; GPG-verified tag (docs/UPSTREAM_SNAPSHOT.md) |
| CPython (via PyInstaller) | 3.14.7 | PSF License (permissive) | |
| PyInstaller | 6.22.3 | GPL-2.0-or-later **with bootloader exception** | The exception explicitly permits bundling arbitrary software; the generated binary is not itself GPL-covered |
| mutagen | 1.48.1 | GPL-2.0-or-later | imported by libmat2 (audio) |
| PyGObject | 3.58.0 | LGPL-2.1-or-later | |
| pycairo | 1.29.1 | LGPL-2.1-or-later OR MPL-1.1 | |
| exiftool | 13.55_1 (Homebrew) | GPL-1.0-or-later OR Artistic (Perl) | bundled script + modules; runs on **system perl** (`/usr/bin/perl5.34`, not bundled) |
| ffmpeg | 9.0.1_1 (Homebrew) | GPL-2.0-or-later (Homebrew build) | standalone CLI binary invoked via argv (aggregation, not linking); sources via Homebrew formula |
| poppler | 26.08.0 | GPL-2.0-or-later | dylibs (PDF) |
| cairo | 1.18.4 | LGPL-2.1-or-later OR MPL-1.1 | |
| glib | 2.90.0 | LGPL-2.1-or-later | |
| gdk-pixbuf | 2.44.8 | LGPL-2.1-or-later | |
| librsvg | 2.63.2 | LGPL-2.1-or-later | |
| gobject-introspection (typelibs) | 1.86.0_3 | LGPL-2.1-or-later | |
| dylib closure (70 libs: freetype, fontconfig, libpng, libjpeg-turbo, libtiff, openjpeg, lcms2, harfbuzz, pango, libwebp, …) | per PACKAGE_INFO | permissive / LGPL-family | full list + hashes in MANIFEST.sha256 |
| synthetic_engine + runtime_entry + mat2_inspect.py | first-party | LicenseRef-Proprietary (owner decision pending) | this project |
| synthetic_metadata_profiles_v1.json | schema 1.0.0 | owner-supplied data pack | SHA-256 pinned: `5bf6b129…efb1d` |

**GPL aggregation note:** the bundle contains GPL components (ffmpeg, poppler,
exiftool, mutagen) exactly as the upstream MAT2 distribution model does
(Debian/Homebrew `mat2` packages depend on the same stack). The proprietary
wrapper communicates with them exclusively via process execution (argv) and
the LGPL libmat2 Python API — no linking. Corresponding-source availability for
GPL components is satisfied by the pinned Homebrew formulae and upstream URLs
recorded in `PACKAGE_INFO.json`.

## 4. External services (no code shipped)

| Service | Interaction | License relevance |
|---|---|---|
| Canarytokens.org (Investigation Tripwire, optional feature) | HTTPS token-creation calls only; **no canarytokens code is bundled** | canarytokens OSS is GPL-3.0-with-thinkst-exceptions; not distributed here — service call only |

## 5. Tooling results (Task 20 gate)

| Tool | Result |
|---|---|
| `cargo test` | 93/93 pass |
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets` | 0 warnings |
| `cargo audit` | 0 vulnerabilities; 2 **allowed warnings**, both platform-conditional and NOT compiled for the macOS target: RUSTSEC-2024-0370 (proc-macro-error, unmaintained — glib-macros build lineage) and RUSTSEC-2024-0429 (glib 0.18.5 unsound `VariantStrIter` — Linux GTK chain `glib←atk←gtk←muda←tauri`). Verified absent from the aarch64-apple-darwin graph (`cargo tree -i glib` → "nothing to print"); `deny.toml` scopes the gate to the shipping target so neither can silently enter it. |
| `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok |
| `npm audit` | 0 vulnerabilities |
| Upstream MAT2 suite (dev env) | 147 tests: 1 failure + 1 skip — **identical to Task 1 baseline** (ffmpeg-version-pinned mp4 expectation; upstream's own glycin skip). Tree restored pristine afterwards (`verify-upstream.sh` OK). |
