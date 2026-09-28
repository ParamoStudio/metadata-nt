#!/usr/bin/env bash
# check-no-html-sinks.sh — Task 16 DOM-injection gate.
#
# Fails if any dynamic-HTML sink appears in frontend CODE (comments
# excluded): innerHTML/outerHTML assignment, insertAdjacentHTML,
# document.write, createContextualFragment, eval, new Function,
# or any network API (fetch/XHR/WebSocket/sendBeacon) — the app is
# offline by design and renders untrusted text via textContent only.

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_SRC="${REPO_ROOT}/app/src"

PATTERN='\.(innerHTML|outerHTML)[[:space:]]*=[^=]|insertAdjacentHTML[[:space:]]*\(|document\.write(ln)?[[:space:]]*\(|createContextualFragment[[:space:]]*\(|\beval[[:space:]]*\(|new[[:space:]]+Function[[:space:]]*\(|\bfetch[[:space:]]*\(|XMLHttpRequest|WebSocket[[:space:]]*\(|sendBeacon[[:space:]]*\('

HITS="$(find "$APP_SRC" "$REPO_ROOT/app/index.html" -type f \( -name '*.ts' -o -name '*.html' -o -name '*.css' \) -print0 \
  | xargs -0 grep -nE "$PATTERN" 2>/dev/null \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(\*|//|/\*)' || true)"

if [ -n "$HITS" ]; then
  echo "FAIL — forbidden dynamic-HTML/network sinks in frontend code:"
  printf '%s\n' "$HITS" | sed 's/^/  /'
  echo
  echo "Policy: untrusted text renders via textContent/createElement only; app is offline (docs/SECURITY.md)."
  exit 1
fi

echo "PASS — no dynamic-HTML sinks, no network APIs in frontend code."
exit 0
