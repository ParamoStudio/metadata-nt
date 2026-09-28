#!/usr/bin/env bash
# check-ipc-surface.sh — Task 16 IPC audit gate.
#
# Asserts the registered Tauri command surface EXACTLY equals the reviewed
# allow-list, and that no generic exec/shell/filesystem/URL command name
# exists anywhere in the Rust source. Adding a command REQUIRES updating
# this allow-list deliberately (and docs/SECURITY.md per the Task 3 gate).

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIB_RS="${REPO_ROOT}/app/src-tauri/src/lib.rs"

ALLOWED=(
  list_selection
  remove_items
  select_files
  select_folder
  choose_output_root
  output_root_info
  start_clean_job
  cancel_job
  set_inplace_armed
  inspect_selection
  open_mat2_site
  open_dangerzone_site
  open_privacytools_site
  open_canarytokens_site
  open_canary_docs
  open_canary_repo
  open_canary_audit
  reveal_output
  runtime_diagnostics
  synthetic_preview
  synthetic_pack_info
  mat2_version
  mat2_formats
  mat2_check_dependencies
  mat2_help
)

FORBIDDEN_NAMES='(^|[^a-z_])(exec|shell|spawn_command|read_file|write_file|delete_file|remove_file_at|open_url|open_path|open_arbitrary|eval|run_command|system)([^a-z_]|$)'

FAILURES=0

[ -f "$LIB_RS" ] || { echo "FAIL: $LIB_RS not found"; exit 1; }

REGISTERED="$(awk '/generate_handler!\[/,/\]\)/' "$LIB_RS" | grep -oE '^[[:space:]]*[a-z_0-9]+,?$' | tr -d ' ,' | sort -u)"
EXPECTED="$(printf '%s\n' "${ALLOWED[@]}" | sort -u)"

MISSING="$(comm -23 <(printf '%s\n' "$EXPECTED") <(printf '%s\n' "$REGISTERED"))"
EXTRA="$(comm -13 <(printf '%s\n' "$EXPECTED") <(printf '%s\n' "$REGISTERED"))"

if [ -n "$MISSING" ]; then
  echo "FAIL — allow-listed commands not registered:"
  printf '%s\n' "$MISSING" | sed 's/^/  /'
  FAILURES=$((FAILURES + 1))
fi
if [ -n "$EXTRA" ]; then
  echo "FAIL — registered commands NOT in the reviewed allow-list (update this script + docs/SECURITY.md after review):"
  printf '%s\n' "$EXTRA" | sed 's/^/  /'
  FAILURES=$((FAILURES + 1))
fi

FORBIDDEN_HITS="$(grep -rnE "#\[tauri::command\]" -A2 "${REPO_ROOT}/app/src-tauri/src/" 2>/dev/null \
  | grep -E "fn [a-z_0-9]+" | grep -iE 'fn (exec|shell|read_file|write_file|delete_file|open_url|open_path|eval|run_command|system)\b' || true)"
if [ -n "$FORBIDDEN_HITS" ]; then
  echo "FAIL — forbidden generic command names present:"
  printf '%s\n' "$FORBIDDEN_HITS" | sed 's/^/  /'
  FAILURES=$((FAILURES + 1))
fi

if [ "$FAILURES" -eq 0 ]; then
  echo "PASS — IPC surface is exactly the reviewed allow-list (${#ALLOWED[@]} commands), no generic exec/shell/fs/url commands."
  exit 0
else
  echo "RESULT: FAILED ($FAILURES) — see docs/SECURITY.md gate rule."
  exit 1
fi
