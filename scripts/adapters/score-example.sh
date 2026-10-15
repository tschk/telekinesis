#!/usr/bin/env bash
set -euo pipefail

dir="${1:?usage: score-example.sh <candidate-dir>}"
avo_dir=$(cd "$(dirname "$0")/../avo" && pwd)
# shellcheck source=../avo/lib.sh
. "$avo_dir/lib.sh"

cd "$dir" || {
  echo "score-example: not a directory: $dir" >&2
  exit 3
}

correct=true
ran_test=false
elapsed=0
test_cmd="${AVO_TEST_CMD:-}"
correct_cmd="${AVO_CORRECT_CMD:-}"

if [ -n "$test_cmd" ]; then
  ran_test=true
  start=$(avo_lua monotonic-time)
  if ! sh -c "$test_cmd" >/dev/null 2>&1; then
    correct=false
  fi
  end=$(avo_lua monotonic-time)
  elapsed=$(avo_lua elapsed "$start" "$end")
elif [ -n "$correct_cmd" ]; then
  if ! sh -c "$correct_cmd" >/dev/null 2>&1; then
    correct=false
  fi
fi

if [ "$correct" != "true" ]; then
  objective=0
  if [ "$ran_test" = true ]; then
    note="tests failed"
  else
    note="correctness command failed"
  fi
elif [ -n "${AVO_OBJECTIVE:-}" ]; then
  objective="$AVO_OBJECTIVE"
  note="${AVO_NOTE:-tests passed}"
elif [ "$ran_test" = true ]; then
  objective=$(avo_lua reciprocal "$elapsed")
  note="${AVO_NOTE:-tests passed}"
else
  objective="${AVO_OBJECTIVE:-0}"
  note="${AVO_NOTE:-example score; set AVO_TEST_CMD / AVO_CORRECT_CMD / AVO_OBJECTIVE / AVO_NOTE}"
fi

export SCORE_CORRECT="$correct"
export SCORE_OBJECTIVE="$objective"
export SCORE_NOTE="$note"
export SCORE_ELAPSED="$elapsed"
export SCORE_RAN_TEST="$ran_test"
avo_lua score-output "$SCORE_CORRECT" "$SCORE_OBJECTIVE" "$SCORE_NOTE" "$SCORE_ELAPSED" "$SCORE_RAN_TEST"
