#!/usr/bin/env bash
# check-no-remote-refs.sh — Task 3 lint gate.
#
# Fails if any remote URL reference (http://, https://, ws://, wss://) appears in
# application source, except the explicit allow-list:
#   1. http://ipc.localhost        — Tauri IPC origin inside the CSP (tauri.conf.json)
#   2. http://127.0.0.1:1420       — local Vite devUrl (tauri.conf.json, dev only)
#   3. The three hard-coded external link constants in app/src-tauri/src/external.rs:
#        https://github.com/jvoisin/mat2
#        https://github.com/freedomofpress/dangerzone
#        https://www.privacytools.io/
#      (file created in Task 14; until then it does not exist and contributes nothing)
#
# Scope: app sources only. docs/, scripts/, lockfiles and node_modules are out of
# scope by design (documentation may cite URLs; lockfiles carry registry metadata).

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

VIOLATIONS="$(grep -rnoE '(https?|wss?)://[^"'"'"' )<>]*' "${EXISTING[@]}" 2>/dev/null \
  | grep -vE 'http://ipc\.localhost' \
  | grep -vE 'http://127\.0\.0\.1:1420' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:https://github\.com/jvoisin/mat2$' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:https://github\.com/freedomofpress/dangerzone$' \
  | grep -vE 'src-tauri/src/external\.rs:[0-9]+:https://www\.privacytools\.io/?$' \
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
