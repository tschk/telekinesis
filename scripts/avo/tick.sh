#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"

AVO_DEFAULT_AGENT=$(cd "$here/../adapters" && pwd)/agent-tk.sh
init=false
allow_dirty=false
AVO_TASK=${AVO_TASK:-}
AVO_GOAL=${AVO_GOAL:-}
AVO_SCORE=${AVO_SCORE:-}
AVO_AGENT=${AVO_AGENT:-}

usage() {
  cat >&2 <<EOF
usage: tick.sh [--init <task>] [--goal <text>] [--score <cmd>] [--agent <cmd>] [--allow-dirty]
one locked AVO variation step: Vary(P_t, K, f) = Agent(...)
never commits to main; never pushes
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --init)
      init=true
      shift
      [ "$#" -gt 0 ] || avo_die "usage: tick.sh --init <task>"
      AVO_TASK=$1
      ;;
    --goal)
      shift
      [ "$#" -gt 0 ] || avo_die "usage: tick.sh --goal <text>"
      AVO_GOAL=$1
      ;;
    --score)
      shift
      [ "$#" -gt 0 ] || avo_die "usage: tick.sh --score <cmd>"
      AVO_SCORE=$1
      ;;
    --agent)
      shift
      [ "$#" -gt 0 ] || avo_die "usage: tick.sh --agent <cmd>"
      AVO_AGENT=$1
      ;;
    --task)
      shift
      [ "$#" -gt 0 ] || avo_die "usage: tick.sh --task <name>"
      AVO_TASK=$1
      ;;
    --allow-dirty)
      allow_dirty=true
      ;;
    --help|-h)
      usage
      exit 2
      ;;
    *)
      usage
      exit 2
      ;;
  esac
  shift
done

if [ "$init" != true ] && [ -z "${AVO_TASK:-}" ] && [ -z "${AVO_GOAL:-}" ] && [ -z "${AVO_SCORE:-}" ] && [ -z "${AVO_AGENT:-}" ] && [ ! -f .avo/current ] && [ ! -d .avo ]; then
  usage
  exit 2
fi

AVO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || avo_die "usage: run from a git repository (or pass --init <task>)"

if [ "$init" = true ]; then
  avo_init
  exit 0
fi

case "$(git -C "$AVO_ROOT" rev-parse --abbrev-ref HEAD)" in
  main|master)
    avo_fail "refusing to run on $(git -C "$AVO_ROOT" rev-parse --abbrev-ref HEAD); use --init <task> to create avo/<task>"
    ;;
esac

avo_load_config
avo_require_branch

if [ "$allow_dirty" != true ] && avo_dirty; then
  avo_fail "refusing dirty tracked files; commit them or pass --allow-dirty"
fi

avo_acquire_lock
trap avo_release_lock EXIT

AVO_START_BRANCH=$(avo_current_branch)
AVO_START_HEAD=$(avo_git rev-parse HEAD)
avo_capture_pre_tick
avo_ensure_baseline

tick=$(avo_tick_number)
prompt="$(avo_task_dir)/prompt.txt"
avo_build_driver_prompt "$prompt"
cp "$prompt" "$AVO_ROOT/.last-prompt"

if ! "$AVO_AGENT" "$AVO_ROOT" "$prompt"; then
  avo_restore_tree
  avo_ledger_append error "$(python3 - "$tick" <<'PY'
import json, sys
print(json.dumps({"tick": int(sys.argv[1]), "correct": False, "objective": 0, "note": "agent failed"}))
PY
)"
  avo_fail "agent failed"
fi

if ! avo_revalidate_git_identity; then
  avo_restore_tree
  avo_ledger_append error "$(python3 - "$tick" <<'PY'
import json, sys
print(json.dumps({"tick": int(sys.argv[1]), "correct": False, "objective": 0, "note": "agent changed branch or HEAD"}))
PY
)"
  avo_fail "agent changed branch or HEAD unexpectedly"
fi

avo_capture_candidate

if ! score_json=$(avo_run_score); then
  avo_persist_candidate "$tick" error
  avo_restore_tree
  avo_ledger_append error "$(python3 - "$tick" "${AVO_CAND_REF:-}" "${AVO_CAND_PATCH:-}" <<'PY'
import json, sys
row = {"tick": int(sys.argv[1]), "correct": False, "objective": 0, "note": "score infra failure"}
if sys.argv[2]:
    row["candidate"] = sys.argv[2]
if sys.argv[3]:
    row["patch"] = sys.argv[3]
print(json.dumps(row))
PY
)"
  avo_fail "score command failed"
fi

correct=$(python3 -c 'import json,sys; print("true" if json.loads(sys.argv[1])["correct"] else "false")' "$score_json")
objective=$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["objective"])' "$score_json")
note=$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["note"])' "$score_json")
stddev=$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["stddev"])' "$score_json")
best=$(avo_best_objective)

payload=$(python3 - "$tick" "$correct" "$objective" "$note" <<'PY'
import json, sys
print(json.dumps({
    "tick": int(sys.argv[1]),
    "correct": sys.argv[2] == "true",
    "objective": float(sys.argv[3]),
    "note": sys.argv[4],
}))
PY
)

if [ "$(avo_should_commit "$correct" "$objective" "$stddev" "$best")" = yes ]; then
  avo_commit_candidate "avo($AVO_TASK): tick $tick objective=$objective"
  commit=$(avo_git rev-parse --short HEAD)
  payload=$(python3 - "$payload" "$commit" <<'PY'
import json, sys
row = json.loads(sys.argv[1])
row["commit"] = sys.argv[2]
row["candidate"] = sys.argv[2]
print(json.dumps(row))
PY
)
  avo_ledger_append accept "$payload"
  AVO_START_HEAD=$(avo_git rev-parse HEAD)
  AVO_START_BRANCH=$(avo_current_branch)
  avo_capture_pre_tick
else
  avo_persist_candidate "$tick" reject
  payload=$(python3 - "$payload" "${AVO_CAND_REF:-}" "${AVO_CAND_PATCH:-}" <<'PY'
import json, sys
row = json.loads(sys.argv[1])
if sys.argv[2]:
    row["candidate"] = sys.argv[2]
if sys.argv[3]:
    row["patch"] = sys.argv[3]
print(json.dumps(row))
PY
)
  avo_restore_tree
  avo_ledger_append reject "$payload"
fi

if [ "$(avo_should_supervise)" = yes ]; then
  avo_run_supervisor
fi
