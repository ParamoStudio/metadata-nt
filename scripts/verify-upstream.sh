#!/usr/bin/env bash
# verify-upstream.sh — read-only integrity check of the supplied MAT2 upstream tree.
#
# Verifies that upstream-mat2/ still matches the snapshot recorded in
# docs/UPSTREAM_SNAPSHOT.md. NEVER modifies the tree (no fetch/checkout/clean).
#
# Exit codes: 0 = all checks pass, 1 = at least one check failed.

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPSTREAM_DIR="${REPO_ROOT}/upstream-mat2"

EXPECTED_REMOTE="https://github.com/jvoisin/mat2"
EXPECTED_HEAD="70c17d3d7b2835e02c7177c6cc58f1911848583c"
EXPECTED_TREE="cd7dacf00812ebfd3e06bf41646d9bf18bc6abb4"
EXPECTED_DESCRIBE="0.15.0-26-g70c17d3"
EXPECTED_TAG="0.15.0"
EXPECTED_TAG_COMMIT="54ba36955d916440b43be5c5ee2a87646b3ca493"
EXPECTED_TAG_KEY="9FCDEE9E1A381F311EA62A7404D041E8171901CC"
EXPECTED_VERSION="0.15.0"

FAILURES=0

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1"; FAILURES=$((FAILURES + 1)); }
warn() { printf 'WARN  %s\n' "$1"; }

echo "== verify-upstream: ${UPSTREAM_DIR} =="

# 1. Directory exists and is a git repository
if [ ! -d "${UPSTREAM_DIR}/.git" ]; then
  fail "upstream-mat2/ is missing or is not a git repository"
  echo "RESULT: FAILED (${FAILURES} check(s))"
  exit 1
fi
pass "upstream-mat2/ exists and is a git repository"

git_upstream() { git -C "${UPSTREAM_DIR}" "$@"; }

# 2. Remote URL
ACTUAL_REMOTE="$(git_upstream remote get-url origin 2>/dev/null || echo '<none>')"
if [ "${ACTUAL_REMOTE}" = "${EXPECTED_REMOTE}" ] || [ "${ACTUAL_REMOTE}" = "${EXPECTED_REMOTE}.git" ]; then
  pass "remote origin = ${ACTUAL_REMOTE}"
else
  fail "remote origin is '${ACTUAL_REMOTE}', expected '${EXPECTED_REMOTE}'"
fi

# 3. HEAD commit
ACTUAL_HEAD="$(git_upstream rev-parse HEAD 2>/dev/null || echo '<none>')"
if [ "${ACTUAL_HEAD}" = "${EXPECTED_HEAD}" ]; then
  pass "HEAD = ${ACTUAL_HEAD}"
else
  fail "HEAD is '${ACTUAL_HEAD}', expected '${EXPECTED_HEAD}'"
fi

# 4. HEAD tree hash (content integrity of the checkout)
ACTUAL_TREE="$(git_upstream rev-parse 'HEAD^{tree}' 2>/dev/null || echo '<none>')"
if [ "${ACTUAL_TREE}" = "${EXPECTED_TREE}" ]; then
  pass "HEAD tree = ${ACTUAL_TREE}"
else
  fail "HEAD tree is '${ACTUAL_TREE}', expected '${EXPECTED_TREE}'"
fi

# 5. Working tree cleanliness (no local modifications to upstream)
DIRTY="$(git_upstream status --porcelain 2>/dev/null)"
if [ -z "${DIRTY}" ]; then
  pass "working tree is clean (no modifications to upstream)"
else
  fail "working tree is dirty:"
  printf '%s\n' "${DIRTY}" | sed 's/^/      /'
fi

# 6. git describe
ACTUAL_DESCRIBE="$(git_upstream describe --tags --always --dirty 2>/dev/null || echo '<none>')"
if [ "${ACTUAL_DESCRIBE}" = "${EXPECTED_DESCRIBE}" ]; then
  pass "describe = ${ACTUAL_DESCRIBE}"
else
  fail "describe is '${ACTUAL_DESCRIBE}', expected '${EXPECTED_DESCRIBE}'"
fi

# 7. Tag commit + signature (signature check is soft: depends on local GPG keyring)
ACTUAL_TAG_COMMIT="$(git_upstream rev-parse "${EXPECTED_TAG}^{commit}" 2>/dev/null || echo '<none>')"
if [ "${ACTUAL_TAG_COMMIT}" = "${EXPECTED_TAG_COMMIT}" ]; then
  pass "tag ${EXPECTED_TAG} points at ${ACTUAL_TAG_COMMIT}"
else
  fail "tag ${EXPECTED_TAG} points at '${ACTUAL_TAG_COMMIT}', expected '${EXPECTED_TAG_COMMIT}'"
fi

if command -v gpg >/dev/null 2>&1; then
  TAG_VERIFY="$(git_upstream tag -v "${EXPECTED_TAG}" 2>&1)"
  if printf '%s' "${TAG_VERIFY}" | grep -q "Good signature"; then
    if printf '%s' "${TAG_VERIFY}" | grep -q "${EXPECTED_TAG_KEY}"; then
      pass "tag ${EXPECTED_TAG} signature: Good signature from expected key ${EXPECTED_TAG_KEY}"
    else
      warn "tag signature good but key fingerprint line not matched; inspect manually"
    fi
  else
    warn "tag ${EXPECTED_TAG} signature could not be verified locally (public key ${EXPECTED_TAG_KEY} not in keyring?). Not a tree-integrity failure."
  fi
else
  warn "gpg not available; skipping tag signature verification"
fi

# 8. Version strings inside the tree
PYPROJECT_VERSION="$(grep -m1 '^version = ' "${UPSTREAM_DIR}/pyproject.toml" 2>/dev/null | sed 's/version = "\(.*\)"/\1/')"
if [ "${PYPROJECT_VERSION}" = "${EXPECTED_VERSION}" ]; then
  pass "pyproject.toml version = ${PYPROJECT_VERSION}"
else
  fail "pyproject.toml version is '${PYPROJECT_VERSION}', expected '${EXPECTED_VERSION}'"
fi

ENTRYPOINT_VERSION="$(grep -m1 "^__version__ = " "${UPSTREAM_DIR}/mat2" 2>/dev/null | sed "s/__version__ = '\(.*\)'/\1/")"
if [ "${ENTRYPOINT_VERSION}" = "${EXPECTED_VERSION}" ]; then
  pass "mat2 entrypoint __version__ = ${ENTRYPOINT_VERSION}"
else
  fail "mat2 entrypoint __version__ is '${ENTRYPOINT_VERSION}', expected '${EXPECTED_VERSION}'"
fi

echo
if [ "${FAILURES}" -eq 0 ]; then
  echo "RESULT: OK — upstream tree matches docs/UPSTREAM_SNAPSHOT.md"
  exit 0
else
  echo "RESULT: FAILED (${FAILURES} check(s)) — do not build against a drifted upstream; re-record the snapshot or restore the pinned commit"
  exit 1
fi
