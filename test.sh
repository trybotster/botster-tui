#!/usr/bin/env bash
set -euo pipefail

export BOTSTER_ENV="${BOTSTER_ENV:-test}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${TMPDIR:-/tmp}/botster-tui-spike-target}"

cargo test "$@"
