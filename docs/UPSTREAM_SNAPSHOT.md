# Upstream MAT2 Snapshot

Recorded: **2026-09-27 20:43 CEST (18:43 UTC)**
Recorded by: implementation agent, per `HANDOFF.md` §4 and `IMPLEMENTATION_PLAN.md` Task 1.

This document is the authoritative record of the exact MAT2 source this wrapper is built
against. The local copy at `upstream-mat2/` wins over any assumption in `HANDOFF.md`,
`INTERFACE.md` or `IMPLEMENTATION_PLAN.md`.

---

## 1. Provenance

| Field | Value |
|---|---|
| Upstream repository URL | `https://github.com/jvoisin/mat2` |
| Supplied commit hash (HEAD) | `70c17d3d7b2835e02c7177c6cc58f1911848583c` |
| Branch | `main` |
| HEAD tree hash | `cd7dacf00812ebfd3e06bf41646d9bf18bc6abb4` |
| `git describe --tags --always --dirty` | `0.15.0-26-g70c17d3` (clean; 26 commits after tag) |
| Nearest tag / release version | `0.15.0` (released 2026-08-04) |
| Tag commit | `54ba36955d916440b43be5c5ee2a87646b3ca493` |
| Working tree state | **clean** (`git status --short` empty) |
| Last commit | Wed Sep 9 2026 — "Update the mailing list link" |
| Version declared in `pyproject.toml` | `0.15.0` |
| Version declared in `mat2` entrypoint (`__version__`) | `0.15.0` |

### Signature information

- The HEAD commit itself is **not** GPG-signed (`%G?` = `N`), which is normal for
  routine post-tag commits on this repository.
- The release tag `0.15.0` **is GPG-signed**:
  - Signature made: Tue Aug 4 16:23:35 2026 CEST
  - Signing key: RSA `9FCDEE9E1A381F311EA62A7404D041E8171901CC`
  - Verification result on this machine (after importing the key from
    `keyserver.ubuntu.com`): **Good signature from "Julien (jvoisin) Voisin
    <julien.voisin@dustri.org>"** (key trust: unknown / not web-of-trust certified).
  - The key fingerprint matches the one published in the upstream `README.md`
    contact section, which is the author's stated security contact key.

### Superseded copies (history note)

- An older copy `mat2-github-sourceode/` (**MAT2 0.9.0, 2019-05-10**, no git metadata)
  was initially supplied by mistake. The owner identified it as deprecated and removed
  it; it was never used as an implementation input. All findings recorded here refer
  to the current upstream.
- `mat2-main/` (a zip-style tree without git metadata) is **byte-identical** to
  `upstream-mat2/` at HEAD (verified with `diff -rq`, only `.DS_Store` differs).
  `upstream-mat2/` (full git clone) is the authoritative copy because it carries the
  commit/tag/signature metadata needed for `scripts/verify-upstream.sh`.

---

## 2. Runtime requirements (from supplied source)

From `pyproject.toml` and `README.md` of the supplied tree:

- **Python**: `requires-python = ">=3.11"`
- **Python package dependencies** (`pyproject.toml`):
  - `mutagen` (audio support)
  - `PyGObject` (GObject introspection bindings)
  - `pycairo` (Cairo bindings, PDF rendering)
- **GObject introspection typelibs required at runtime** (from `libmat2/__init__.py`
  `DEPENDENCIES` and README):
  - `Poppler-0.18` (PDF) — required
  - `GdkPixbuf-2.0` (images) — required
  - `GLib-2.0` — required
  - `Rsvg-2.0` (SVG) — required per README (`gir1.2-rsvg-2.0`)
- **Optional external commands** (from `libmat2/__init__.py` `CMD_DEPENDENCIES`):
  - `exiftool` (`libimage-exiftool-perl`) — optional, "everything else" formats
  - `ffmpeg` — optional, video support
- **Removed since 0.14.0**: bubblewrap sandboxing (Linux-only). The `--no-sandbox`
  flag still parses but is **deprecated and non-operative** (emits a
  `DeprecationWarning`: "sandboxing support has been removed").
- macOS dependency guidance (README):
  `brew install exiftool cairo pygobject3 poppler gdk-pixbuf librsvg ffmpeg`
  (plus `webp-pixbuf-loader` on macOS for WebP image loading via GdkPixbuf — see §5).

### Development environment actually used for this snapshot

| Component | Version |
|---|---|
| macOS | 27.0 (build 26A428), arm64 (Apple Silicon) |
| Python (venv, `--system-site-packages` over Homebrew python) | 3.14.7 |
| PyGObject (`gi`) | 3.58.0 (Homebrew `pygobject3`) |
| pycairo | 1.29.1 (Homebrew `py3cairo`) |
| mutagen | 1.48.1 (pip, into project `.venv/`) |
| exiftool | 13.55 (Homebrew) |
| ffmpeg | 9.0.1 (Homebrew) |
| poppler / gdk-pixbuf / librsvg / gobject-introspection / cairo | current Homebrew bottles |
| webp-pixbuf-loader | 0.2.7 (Homebrew; needed for `.webp` via GdkPixbuf on macOS) |

MAT2 is invoked from the supplied tree directly:
`<project>/.venv/bin/python <project>/upstream-mat2/mat2 …`
(no global/pip-installed MAT2 is used or required).

---

## 3. Exact CLI surface (captured from the supplied entrypoint)

### `mat2 --version`

```text
mat2 0.15.0
```

### `mat2 --help` (verbatim)

```text
usage: mat2 [-h] [-V] [--unknown-members policy] [--inplace] [--no-sandbox]
            [-L | -s]
            [-v | -l | --check-dependencies | files ...]

Metadata anonymisation toolkit 2

positional arguments:
  files                 the files to process

options:
  -h, --help            show this help message and exit
  -V, --verbose         show more verbose status information
  --unknown-members policy
                        how to handle unknown members of archive-style files
                        (policy should be one of: abort, omit, keep) [Default:
                        abort]
  --inplace             clean in place, without backup
  --no-sandbox          Disable bubblewrap's sandboxing
  -v, --version         show program's version number and exit
  -l, --list            list all supported fileformats
  --check-dependencies  check if mat2 has all the dependencies it needs
  -L, --lightweight     remove SOME metadata
  -s, --show            list harmful metadata detectable by mat2 without
                        removing them
```

### `mat2 --check-dependencies` (verbatim, this machine)

```text
Dependencies for mat2 0.15.0:
- Cairo: yes
- Exiftool: yes (optional)
- Ffmpeg: yes (optional)
- GLib from PyGobject: yes
- GdkPixbuf from PyGobject: yes
- Mutagen: yes
- Poppler from PyGobject: yes
- PyGobject: yes
```

(Trailing spaces present in original output trimmed here; Exiftool/Ffmpeg lines carry
` (optional)` suffix.)

### `mat2 --list` (verbatim, this machine)

```text
[+] Supported formats:
  - application/epub+zip (.epub)
  - application/pdf (.pdf)
  - application/vnd.oasis.opendocument.chart (.odc)
  - application/vnd.oasis.opendocument.formula (.odf)
  - application/vnd.oasis.opendocument.graphics (.odg)
  - application/vnd.oasis.opendocument.image (.odi)
  - application/vnd.oasis.opendocument.presentation (.odp)
  - application/vnd.oasis.opendocument.spreadsheet (.ods)
  - application/vnd.oasis.opendocument.text (.odt)
  - application/vnd.openxmlformats-officedocument.presentationml.presentation (.pptx)
  - application/vnd.openxmlformats-officedocument.spreadsheetml.sheet (.xlsx)
  - application/vnd.openxmlformats-officedocument.wordprocessingml.document (.docx)
  - application/x-bittorrent (.torrent)
  - application/x-dtbncx+xml (.ncx)
  - application/x-tar (.tar)
  - application/xhtml+xml (.xht, .xhtml)
  - application/zip (.zip)
  - audio/flac (.flac)
  - audio/mpeg (.m3a, .m2a, .mpga, .mp3, .mp2a, .mp2)
  - audio/ogg (.oga, .opus, .spx, .ogg)
  - audio/x-aiff (.aiff, .aif, .aifc)
  - audio/x-wav (.wav)
  - image/avif (.avif)
  - image/bmp (.bmp)
  - image/gif (.gif)
  - image/heic (.heic)
  - image/jpeg (.jpg, .jpeg, .jpe)
  - image/jxl (.jxl)
  - image/png (.png)
  - image/svg+xml (.svgz, .svg)
  - image/tiff (.tiff, .tif)
  - image/webp (.webp)
  - image/x-portable-pixmap (.ppm)
  - text/css (.css)
  - text/html (.htm, .html)
  - text/plain (.log, .text, .txt, .list, .in, .def, .conf)
  - video/mp4 (.mp4, .m4v, .mpg4, .mp4v)
  - video/x-ms-wmv (.wmv)
  - video/x-msvideo (.avi)
```

Note: exact extension lists are produced via Python `mimetypes.guess_all_extensions`
minus `libmat2.UNSUPPORTED_EXTENSIONS`, so they can vary slightly by platform
mimetypes database. The wrapper must treat `--list` output at runtime as the authority,
not this capture.

---

## 4. Behavioral facts verified against the supplied source (wrapper-critical)

Read from `upstream-mat2/mat2` at HEAD and verified live where noted:

1. **Output convention (normal mode)**: produces `<name>.cleaned.<ext>` next to the
   source; the original is never modified. Verified live (PNG fixture).
2. **`--inplace` exists**: cleans in place without backup (`os.rename` of the cleaned
   file over the original). Destructive-mode rules from `HANDOFF.md` §9.5 /
   `INTERFACE.md` §22 apply.
3. **`--no-sandbox` is deprecated/non-operative** (removed sandboxing in 0.14.0);
   it only triggers a `DeprecationWarning`. → Document, do **not** expose in UI
   (Task 13 category: deprecated/non-operative).
4. **`--` end-of-options works** (argparse built-in): verified live by cleaning
   `-dash-name.png` passed as `-- ./-dash-name.png`. The runner must always pass
   `--` before file arguments.
5. **Exit codes**:
   - clean mode: `0` if all files succeeded, `-1` (i.e. `255` unsigned) if any failed;
   - `-s/--show` mode: **always `0`**, even for unsupported/missing files (it prints
     `[-] …` messages instead). The wrapper must parse `--show` output and must not
     infer support/failure from the exit code in show mode.
   - `--list`, `--check-dependencies`, `--version`, no-args help: `0`.
6. **Unsupported file message** (clean & show): `[-] <file>'s format (<mimetype>) is
   not supported`. Missing file: `[-] <file> doesn't exist.` Non-regular file:
   `[-] <file> is not a regular file.` Unreadable: `[-] <file> is not readable.`
7. **`--show` output shape** (for the inspection parser, Task 9):
   - Header: `[+] Metadata for <file>:`
   - Entries: `  <key>: <value>` (2-space indent per depth level; nested dicts for
     archive members produce `[+++] Metadata for <member>:` blocks at depth+1)
   - Clean file: `  No metadata found in <file>.`
   - The CLI strips Unicode category `C*` (control/format) characters from values
     before printing; values that fail encoding print as `harmful content`.
8. **Verbose (`-V`)**: raises logging to DEBUG on the `mat2` logger (stderr, format
   `%(levelname)s: %(message)s`).
9. **Parallelism**: clean mode uses `ProcessPoolExecutor` internally over the given
   file list. The wrapper invokes MAT2 **one file per invocation** (sequential v1
   model), so this does not affect job semantics; cancellation kills a single-file
   child process.
10. **Directories**: MAT2 recursively walks directories given as arguments. The
    wrapper must pass **explicit regular-file paths only** (never directories), per
    `HANDOFF.md` §16.
11. **Symlinks**: `os.path.isfile()` follows symlinks, so MAT2 itself will clean a
    symlink-to-regular-file (operating on the target's parser, writing
    `<linkname>.cleaned.<ext>`). Wrapper policy (HANDOFF §16): enumerate regular
    files, do not follow symlinked directories; symlinked *files* classification is
    explicit in the selection registry (Task 4 test).
12. **Unknown-member policies**: `abort` (default), `omit`, `keep`; `keep` logs a
    warning that it may leak metadata.
13. **Lightweight (`-L`)** and `--show` (`-s`) are mutually exclusive; `-v/-l/
    --check-dependencies/files` are mutually exclusive.

---

## 5. Upstream test suite result on this machine

Command (upstream-documented): `python3 -m unittest discover -v`
Run from `upstream-mat2/` with `.venv/bin` first on `PATH` (so the `./mat2` shebang
`/usr/bin/env python3` resolves to a Python that has all dependencies).

### Result

```text
Ran 147 tests in 36.151s
FAILED (failures=1, skipped=1)
```

- **146/147 pass** on macOS 27.0 arm64, Python 3.14.7, exiftool 13.55, ffmpeg 9.0.1.
- **1 failure — environmental, not a sanitisation defect**:
  `tests.test_libmat2.TestCleaning.test_all_parametred (case: mp4)` — after cleaning,
  the mp4 `TimeScale` box value is `720000` with ffmpeg 9.0.1 while the test pins the
  value `1000` produced by the older ffmpeg used in upstream CI (Debian baseline).
  Cleaning itself succeeds; the assertion is an exact-value pin on ffmpeg output.
- **1 skip — upstream's own**: `tests.test_climat2.TestControlCharInjection.test_jpg`
  skipped with reason `"glycin doesn't like some jpg"` (skip is in upstream code).

### Environment notes discovered while running the suite

- A first run without the venv on `PATH` produced 21 failures / 4 errors, all traced
  to two environment gaps, both resolved before recording the result above:
  1. `./mat2` subprocesses used Homebrew `python3` **without mutagen** → fixed by
     putting `.venv/bin` first on `PATH` (the venv uses `--system-site-packages` to
     inherit Homebrew `gi`/`cairo` and adds `mutagen` via pip).
  2. WebP loading failed (`gdk-pixbuf-error-quark: Couldn't recognize the image file
     format`) because macOS gdk-pixbuf lacks a WebP loader → fixed by
     `brew install webp-pixbuf-loader`.
- The suite leaves untracked residue in `tests/data/` (e.g. `clean.cleaned.docx`,
  `clean.cleaned.mp4`, `clean.mp4` from the failing mp4 case). These generated files
  were removed after the run to restore the pristine supplied state; tracked upstream
  content was never modified (guaranteed by the HEAD tree-hash check).
  `scripts/verify-upstream.sh` enforces a clean upstream tree and never modifies it.
  Re-running the upstream suite will dirty the tree again — clean the residue (or
  re-run the verifier) before release packaging.

---

## 6. Material differences vs HANDOFF.md assumptions

Per `HANDOFF.md` §4 "Important rule", differences are documented here and the wrapper
mapping adapts to the supplied upstream:

| HANDOFF.md said (at preparation time) | Supplied upstream actually is | Wrapper consequence |
|---|---|---|
| "Python 3.11+" | `>=3.11` — matches | none |
| Deps "Mutagen, Poppler/Cairo, GdkPixbuf, librsvg, FFmpeg and ExifTool" | matches (bubblewrap sandboxing removed in 0.14.0) | `--no-sandbox` documented as non-operative, not exposed |
| "normal cleaning produces `.cleaned` file" | matches | none |
| "lightweight mode", "show mode", "archive unknown-member policies", "dependency/format diagnostics" | all present, exact spellings captured in §3 | option mapping uses captured CLI verbatim |
| in-place "if upstream still exposes it" | **`--inplace` exists** | Advanced destructive mode implemented per INTERFACE.md §22 (off every launch, double confirmation) |

No material difference blocks the plan. No upstream modification was made or is needed.

---

## 7. Option-parity seed (feeds Task 13)

Every option in the supplied `mat2 --help` and its planned disposition:

| Upstream option | Category | Planned wrapper disposition |
|---|---|---|
| `files` (positional) | processing | explicit selected file list, always after `--` |
| `-L, --lightweight` | processing option | "Lightweight" cleaning mode radio |
| `-s, --show` | processing option | pre/post inspection + "Inspect only" action |
| `-V, --verbose` | processing option | Advanced: "Verbose MAT2 output" checkbox |
| `--unknown-members {abort,omit,keep}` | processing option | Advanced: dropdown, default `abort`; `keep` shows leak warning |
| `--inplace` | processing option (destructive) | Advanced: "Replace original files", off every launch, double confirmation |
| `-v, --version` | diagnostic | Advanced: "Show MAT2 version" + startup diagnostics |
| `-l, --list` | diagnostic | Advanced: "List supported formats" |
| `--check-dependencies` | diagnostic | Advanced: "Check dependencies" + startup gate |
| `-h, --help` | diagnostic | Advanced: "Show MAT2 help" |
| `--no-sandbox` | deprecated/non-operative | **not exposed**; documented here only |
