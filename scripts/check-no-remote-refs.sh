#!/usr/bin/env bash
# check-no-remote-refs.sh — Task 3 lint gate.
#
# Fails if any remote URL reference (http://, https://, ws://, wss://) appears in
# application source, except the explicit allow-list:
#   1. http://ipc.localhost        — Tauri IPC origin inside the CSP (tauri.conf.json)
#   2. http://127.0.0.1:1420       — local Vite devUrl (tauri.conf.json, dev only)
#   3. The seven hard-coded external link constants in app/src-tauri/src/external.rs:
#        https://github.com/jvoisin/mat2
#        https://github.com/freedomofpress/dangerzone
#        https://www.privacytools.io/
#        https://canarytokens.org/                          (tripwire info dialog)
#        https://docs.canarytokens.org/guide/fast-redirect-token.html
#        https://github.com/thinkst/canarytokens
#        https://resources.canary.tools/documents/Doyensec_ThinkstCanaryTokensOSS_Report_Q22024_WithRetesting.pdf
#   4. The hard-coded Canarytokens API origin in app/src-tauri/src/tripwire.rs
#      (the ONLY intentional network path; owner-approved add-on spec §16).
#   5. `https://archive.org/` — the spec-mandated DEFAULT redirect destination
#      value (tripwire spec §11); it is data sent to the canary service when
#      the user enables the tripwire, never fetched by the app.
#   6. Bare scheme literals `"https://"` used in redirect-validation code
#      (scheme prefix checks, not URLs).
#
# Scope: app PRODUCTION sources only. docs/, scripts/, lockfiles and
# node_modules are out of scope by design (documentation may cite URLs;
# lockfiles carry registry metadata). Rust `#[cfg(test)]` modules are also out
# of scope: test fixtures may contain URL strings as hostile-input data (e.g.
# log_sanitize tests proving links are never auto-created); test code never
# ships in release binaries. Everything from the first `#[cfg(test)]` marker
# to EOF in each .rs file is stripped before scanning.

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="${REPO_ROOT}/app"

SCAN_PATHS=(
  "${APP_DIR}/index.html"
  "${APP_DIR}/src"
  "${APP_DIR}/src-tauri/src"
  "${APP_DIR}/src-tauri/capabilities"
  "${APP_DIR}/src-tauri/tauri.conf.json"
  "${APP_DIR}/src-tauri/build.rs"
  "${APP_DIR}/vite.config.ts"
  "${APP_DIR}/tsconfig.json"
  "${APP_DIR}/package.json"
)

EXISTING=()
for p in "${SCAN_PATHS[@]}"; do
  [ -e "$p" ] && EXISTING+=("$p")
done

if [ ${#EXISTING[@]} -eq 0 ]; then
  echo "check-no-remote-refs: nothing to scan (app/ not scaffolded yet?)"
  exit 0
fi

TMP_SCAN="$(mktemp -d)"
trap 'rm -rf "${TMP_SCAN}"' EXIT

SCAN_LIST="${TMP_SCAN}/files.txt"
: > "${SCAN_LIST}"
while IFS= read -r -d '' f; do
  case "$f" in
    *.rs)
      # strip the test module (from first #[cfg(test)] to EOF), keep a stable
      # path-prefixed stream for reporting
      rel="${f#"${APP_DIR}/"}"
      awk -v rel="${rel}" '
        /^#\[cfg\(test\)\]/ { intest=1 }
        !intest { print rel ":" FNR ":" $0 }
      ' "$f" >> "${SCAN_LIST}"
      ;;
    *)
      rel="${f#"${APP_DIR}/"}"
      grep -n '' "$f" 2>/dev/null | sed "s|^|${rel}:|" >> "${SCAN_LIST}" || true
      ;;
  esac
done < <(find "${EXISTING[@]}" -type f \( -name '*.rs' -o -name '*.ts' -o -name '*.html' -o -name '*.css' -o -name '*.json' \) -not -path '*/node_modules/*' -print0 2>/dev/null)

VIOLATIONS="$(grep -E '(https?|wss?)://' "${SCAN_LIST}" 2>/dev/null \
  | grep -vE 'http://ipc\.localhost' \
  | grep -vE 'http://127\.0\.0\.1:1420' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:.*https://github\.com/jvoisin/mat2"' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:.*https://github\.com/freedomofpress/dangerzone"' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:.*https://www\.privacytools\.io/?"' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:.*https://canarytokens\.org/"' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:.*https://docs\.canarytokens\.org/guide/fast-redirect-token\.html"' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:.*https://github\.com/thinkst/canarytokens"' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:.*https://resources\.canary\.tools/documents/Doyensec_ThinkstCanaryTokensOSS_Report_Q22024_WithRetesting\.pdf"' \
  | grep -vE 'src-tauri/src/tripwire\.rs:[0-9]+:.*https://canarytokens\.org' \
  | grep -vE 'https://archive\.org/' \
  | grep -vE 'https://"' \
  | grep -vE 'https:// ' \
  || true)"

if [ -n "${VIOLATIONS}" ]; then
  echo "FAIL — remote URL references found in app source (only the allow-list is permitted):"
  printf '%s\n' "${VIOLATIONS}" | sed 's/^/  /'
  echo
  echo "Policy: docs/SECURITY.md — the app is offline by design; external links are"
  echo "three hard-coded Rust constants in src-tauri/src/external.rs."
  exit 1
fi

echo "PASS — no remote URL references outside the allow-list."
exit 0
