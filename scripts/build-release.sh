#!/usr/bin/env bash
# build-release.sh — public release build with build-host path remapping.
#
# Rust embeds compile-time absolute paths in panic locations; a public
# binary must not reveal the build machine's user directory. Remap the
# home directory and the workspace root to neutral prefixes so released
# artifacts carry no build-host metadata.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export RUSTFLAGS="--remap-path-prefix=${HOME}/=home/ --remap-path-prefix=${REPO_ROOT}/=src/"

cd "${REPO_ROOT}/app"
npm run tauri build
