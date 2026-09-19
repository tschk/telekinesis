#!/usr/bin/env bash
# Deterministic CLI corpus for PGO/PGSO training. No network, no TUI.
# Usage: train.sh PATH_TO_TK
set -euo pipefail

TK="${1:-}"
if [[ -z "$TK" || ! -x "$TK" ]]; then
  echo "usage: scripts/pgo/train.sh PATH_TO_TK" >&2
  exit 2
fi

run() {
  # Instrumented runs must finish; CLI error paths are still useful samples.
  # stdin is /dev/null so `tk exec` never blocks waiting for a prompt.
  "$TK" "$@" </dev/null >/dev/null 2>&1 || true
}

run --version
run --help
run -h
run exec --help
run acp --help
run exec
run exec --json --cwd "${TMPDIR:-/tmp}"
run exec --no-yolo --help
run --nope
printf '' | "$TK" exec - >/dev/null 2>&1 || true
printf '%s\n' '' | "$TK" exec - >/dev/null 2>&1 || true
