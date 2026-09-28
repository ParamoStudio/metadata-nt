#!/usr/bin/env bash
# package-mat2-macos.sh — Task 19: build & verify the self-contained MAT2
# runtime for the macOS application bundle.
#
# Produces:   app/src-tauri/resources/mat2-runtime/  (PyInstaller onedir)
# Artifacts:  build/runtime-dist/MANIFEST.sha256, build/runtime-dist/PACKAGE_INFO.json
#
# Release gate (IMPLEMENTATION_PLAN Task 19): the script FAILS unless the
# frozen runtime passes the full clean-env verification battery. A failed
# build must never be labeled a complete public release; the documented
# developer build (project .venv + Homebrew deps) remains the fallback.
#
# No MAT2 sanitisation logic is modified: the bundled upstream tree is the
# verified snapshot (docs/UPSTREAM_SNAPSHOT.md), pruned only of tests/CI/
# desktop-integration files; the frozen entry (scripts/packaging/
# runtime_entry.py) calls upstream's own functions.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${REPO_ROOT}/build"
UPSTREAM="${REPO_ROOT}/upstream-mat2"
STAGE="${BUILD_DIR}/stage"
STAGELIB="${BUILD_DIR}/stagelib"
SRC_UPSTREAM="${BUILD_DIR}/upstream"
DIST="${BUILD_DIR}/runtime-dist"
DEST="${REPO_ROOT}/app/src-tauri/resources/mat2-runtime"
VENV="${BUILD_DIR}/venv"

EXPECTED_VERSION_LINE="mat2 0.15.0"
SMOKE_FORMATS=(dirty.jpg dirty.png dirty.pdf dirty.docx dirty.mp3 dirty.svg dirty.tiff dirty.ogg dirty.flac dirty.bmp dirty.gif dirty.epub dirty.webp)

log() { printf '\n=== %s ===\n' "$1"; }

# ---------------------------------------------------------------- Stage 0
log "Stage 0: upstream integrity"
"${REPO_ROOT}/scripts/verify-upstream.sh"
UPSTREAM_COMMIT="$(git -C "$UPSTREAM" rev-parse HEAD)"

command -v otool >/dev/null || { echo "FAIL: Xcode command line tools required"; exit 1; }
command -v rsync >/dev/null || { echo "FAIL: rsync required"; exit 1; }

BREW_PREFIX="$(brew --prefix 2>/dev/null || echo /opt/homebrew)"
for req in lib/libpoppler-glib.8.dylib lib/librsvg-2.2.dylib bin/ffmpeg opt/exiftool/libexec/bin/exiftool; do
  [ -e "${BREW_PREFIX}/${req}" ] || { echo "FAIL: missing ${BREW_PREFIX}/${req} — see README 'Requirements setup on macOS'"; exit 1; }
done

# ---------------------------------------------------------------- Stage 1
log "Stage 1: build venv (pyinstaller)"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR" "$STAGE/bin" "$STAGE/exiftool/lib" "$SRC_UPSTREAM"
python3 -m venv --system-site-packages "$VENV"
"$VENV/bin/pip" install --quiet --disable-pip-version-check pyinstaller mutagen
PYINSTALLER_VERSION="$("$VENV/bin/pyinstaller" --version)"
PYTHON_VERSION="$("$VENV/bin/python" --version 2>&1)"
"$VENV/bin/python" -c "import gi, cairo, mutagen" || { echo "FAIL: build venv cannot see gi/cairo/mutagen (install Homebrew deps per upstream README)"; exit 1; }

# ---------------------------------------------------------------- Stage 2
log "Stage 2: staged inputs"
rsync -a --exclude '.git' --exclude 'tests' --exclude 'dolphin' --exclude 'nemo' \
  --exclude 'data' --exclude '.gitlab-ci.yml' "$UPSTREAM/" "$SRC_UPSTREAM/"
cp "${REPO_ROOT}/app/src-tauri/resources/mat2_inspect.py" "${BUILD_DIR}/mat2_inspect.py"
cp "${REPO_ROOT}/scripts/packaging/runtime_entry.py" "${BUILD_DIR}/runtime_entry.py"

EX_LIBEXEC="${BREW_PREFIX}/opt/exiftool/libexec"
cp "${EX_LIBEXEC}/bin/exiftool" "${STAGE}/exiftool/exiftool"
cp -R "${EX_LIBEXEC}/lib/perl5/"* "${STAGE}/exiftool/lib/"
cat > "${STAGE}/bin/exiftool" <<'SHIM'
#!/bin/sh
SELF=$0
DIR=${SELF%/*}
for P in /usr/bin/perl5.34 /usr/bin/perl; do
  [ -x "$P" ] && exec "$P" "$DIR/../exiftool/exiftool" "$@"
done
echo "exiftool shim: no system perl found" >&2
exit 1
SHIM
chmod +x "${STAGE}/bin/exiftool"

log "Stage 2b: dylib closure (poppler, rsvg, ffmpeg)"
python3 "${REPO_ROOT}/scripts/packaging/collect_dylibs.py" "$STAGELIB" \
  "${BREW_PREFIX}/lib/libpoppler-glib.8.dylib" \
  "${BREW_PREFIX}/lib/librsvg-2.2.dylib" \
  "${BREW_PREFIX}/bin/ffmpeg"

GI_CAIRO="$("$VENV/bin/python" - <<'PY'
import gi, os, sys
d = os.path.dirname(gi.__file__)
matches = sorted(f for f in os.listdir(d) if f.startswith('_gi_cairo') and f.endswith('.so'))
if not matches:
    sys.exit('gi/_gi_cairo*.so not found in %s' % d)
print(os.path.join(d, matches[0]))
PY
)" || { echo "FAIL: cannot locate gi/_gi_cairo for the build interpreter"; exit 1; }
echo "gi_cairo: ${GI_CAIRO}"

# ---------------------------------------------------------------- Stage 3
log "Stage 3: PyInstaller freeze"
cd "$BUILD_DIR"
PYTHONPATH="$SRC_UPSTREAM" "$VENV/bin/pyinstaller" \
  --onedir --name mat2-runtime \
  --paths "$SRC_UPSTREAM" \
  --collect-submodules libmat2 \
  --add-binary "${GI_CAIRO}:gi" \
  --add-data "${SRC_UPSTREAM}:upstream" \
  --add-data "${BUILD_DIR}/mat2_inspect.py:." \
  --add-data "${STAGE}/exiftool:exiftool" \
  --add-data "${STAGE}/bin/exiftool:bin" \
  --add-data "${STAGELIB}:." \
  --add-binary "${BREW_PREFIX}/bin/ffmpeg:." \
  --distpath "$DIST" --workpath "${BUILD_DIR}/work" --specpath "${BUILD_DIR}/work" \
  --log-level ERROR \
  runtime_entry.py

RT="${DIST}/mat2-runtime/mat2-runtime"
[ -x "$RT" ] || { echo "FAIL: frozen runtime binary missing"; exit 1; }

# ---------------------------------------------------------------- Stage 4
log "Stage 4: clean-env verification battery"
# env -i simulates a Finder-launched app: no Homebrew in PATH, no user env.
CLEAN_ENV=(env -i HOME="$HOME")

PROBE="$("${CLEAN_ENV[@]}" "$RT" probe 2>/dev/null)" || { echo "FAIL: probe:${PROBE}"; exit 1; }
echo "$PROBE"
echo "$PROBE" | grep -q '"cairo_foreign": "ok"' || { echo "FAIL: cairo foreign not registered"; exit 1; }
echo "$PROBE" | grep -q '"rsvg_render": "ok"' || { echo "FAIL: rsvg render"; exit 1; }
echo "$PROBE" | grep -q '"poppler": "ok' || { echo "FAIL: poppler symbols"; exit 1; }

VERSION_OUT="$("${CLEAN_ENV[@]}" "$RT" mat2 --version)"
[ "$VERSION_OUT" = "$EXPECTED_VERSION_LINE" ] || { echo "FAIL: version '${VERSION_OUT}' != '${EXPECTED_VERSION_LINE}'"; exit 1; }
echo "version OK: ${VERSION_OUT}"

DEPS_OUT="$("${CLEAN_ENV[@]}" "$RT" mat2 --check-dependencies)"
echo "$DEPS_OUT"
MISSING_REQUIRED="$(echo "$DEPS_OUT" | grep ': no' | grep -v '(optional)' || true)"
[ -z "$MISSING_REQUIRED" ] || { echo "FAIL: required deps missing: ${MISSING_REQUIRED}"; exit 1; }
echo "$DEPS_OUT" | grep -q 'Exiftool: yes' || echo "WARN: exiftool not bundled/detected — exiftool-dependent formats degraded"
echo "$DEPS_OUT" | grep -q 'Ffmpeg: yes' || echo "WARN: ffmpeg not bundled/detected — video formats degraded"

LIST_OUT="$("${CLEAN_ENV[@]}" "$RT" mat2 --list)"
echo "$LIST_OUT" | grep -q 'image/jpeg' || { echo "FAIL: format list broken"; exit 1; }

SMOKE_DIR="$(mktemp -d /tmp/mat2pkg-smoke.XXXXXX)"
trap 'rm -rf "$SMOKE_DIR"' EXIT
FAILURES=0
for f in "${SMOKE_FORMATS[@]}"; do
  cp "${UPSTREAM}/tests/data/${f}" "${SMOKE_DIR}/"
done
cp "${UPSTREAM}/tests/data/dirty.mp4" "${SMOKE_DIR}/" 2>/dev/null || true
cd "$SMOKE_DIR"
for f in "${SMOKE_FORMATS[@]}" dirty.mp4; do
  [ -f "$f" ] || continue
  if "${CLEAN_ENV[@]}" "$RT" mat2 -- "$f" > "smoke-${f}.log" 2>&1; then
    echo "PASS clean $f"
  else
    echo "FAIL clean $f:"; tail -2 "smoke-${f}.log"; FAILURES=$((FAILURES+1))
  fi
done
# structured inspection roundtrip on the cleaned jpeg
"${CLEAN_ENV[@]}" "$RT" inspect bundled dirty.cleaned.jpg | grep -q '"ok": true' \
  || { echo "FAIL: frozen inspect adapter"; FAILURES=$((FAILURES+1)); }
echo "PASS inspect adapter (frozen)"
[ "$FAILURES" -eq 0 ] || { echo "FAIL: ${FAILURES} smoke failures"; exit 1; }
cd "$REPO_ROOT"

# ---------------------------------------------------------------- Stage 5
log "Stage 5: manifest, hashes, licenses capture"
cd "${DIST}/mat2-runtime"
find . -type f -exec shasum -a 256 {} \; | sort -k2 > "${DIST}/MANIFEST.sha256"
cd "$REPO_ROOT"
BREW_VERSIONS="$(brew list --versions exiftool ffmpeg poppler librsvg gdk-pixbuf cairo glib gobject-introspection pygobject3 py3cairo webp-pixbuf-loader 2>/dev/null || true)"
cat > "${DIST}/PACKAGE_INFO.json" <<EOF
{
  "upstream_repository": "https://github.com/jvoisin/mat2",
  "upstream_commit": "${UPSTREAM_COMMIT}",
  "upstream_version": "0.15.0",
  "mat2_python": "${PYTHON_VERSION}",
  "pyinstaller": "${PYINSTALLER_VERSION}",
  "built_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "built_on": "$(sw_vers -productVersion) $(uname -m)",
  "bundle_size_bytes": $(du -sk "${DIST}/mat2-runtime" | awk '{print $1 * 1024}'),
  "pruned_from_upstream": ["tests/", "dolphin/", "nemo/", "data/", ".gitlab-ci.yml", ".git/"],
  "brew_components": $(echo "$BREW_VERSIONS" | python3 -c "import json,sys; print(json.dumps(dict(l.rsplit(' ',1) for l in sys.stdin.read().strip().splitlines() if l)))")
}
EOF
{
  echo "# Third-party components captured at packaging time ($(date -u +%Y-%m-%dT%H:%M:%SZ))"
  echo
  echo "## Frozen runtime components"
  echo '```'
  echo "$BREW_VERSIONS"
  echo "pyinstaller ${PYINSTALLER_VERSION}"
  echo "${PYTHON_VERSION}"
  echo '```'
  echo
  echo "Upstream MAT2 LICENSE (LGPL-3.0-or-later) is bundled at _internal/upstream/LICENSE."
  echo "Full license texts consolidated in docs/THIRD_PARTY_LICENSES.md (Task 20)."
} > "${DIST}/LICENSES_CAPTURE.txt"

# ---------------------------------------------------------------- Stage 6
log "Stage 6: install into app resources"
rm -rf "$DEST"
mkdir -p "$DEST"
rsync -a "${DIST}/mat2-runtime/" "$DEST/"
echo "Installed: ${DEST}"
echo
echo "PACKAGING OK — frozen runtime verified in clean env."
echo "Manifest:  ${DIST}/MANIFEST.sha256 ($(wc -l < "${DIST}/MANIFEST.sha256" | tr -d ' ') files)"
echo "Info:      ${DIST}/PACKAGE_INFO.json"
