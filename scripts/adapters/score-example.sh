#!/usr/bin/env bash
set -euo pipefail

dir="${1:?usage: score-example.sh <candidate-dir>}"
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
  start=$(python3 -c 'import time; print(time.time())')
  if ! sh -c "$test_cmd" >/dev/null 2>&1; then
    correct=false
  fi
  end=$(python3 -c 'import time; print(time.time())')
  elapsed=$(python3 -c 'import sys; print(max(float(sys.argv[2]) - float(sys.argv[1]), 1e-6))' "$start" "$end")
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
  objective=$(python3 -c 'import sys; print(1.0 / float(sys.argv[1]))' "$elapsed")
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
python3 - <<'PY'
import json, os
metrics = {}
if os.environ["SCORE_RAN_TEST"] == "true":
    metrics["elapsed_s"] = float(os.environ["SCORE_ELAPSED"])
print(json.dumps({
    "correct": os.environ["SCORE_CORRECT"] == "true",
    "objective": float(os.environ["SCORE_OBJECTIVE"]),
    "metrics": metrics,
    "note": os.environ["SCORE_NOTE"],
    "artifacts": [],
}))
PY
