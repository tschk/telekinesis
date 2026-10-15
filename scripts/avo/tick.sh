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

avo_pin_score

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
  payload=$(avo_lua json-object "n:tick=$tick" "b:correct=false" "n:objective=0" "s:note=agent failed")
  avo_ledger_append error "$payload"
  avo_maybe_supervise
  avo_fail "agent failed"
fi

if ! avo_revalidate_git_identity; then
  avo_restore_tree
  payload=$(avo_lua json-object "n:tick=$tick" "b:correct=false" "n:objective=0" "s:note=agent changed branch or HEAD")
  avo_ledger_append error "$payload"
  avo_maybe_supervise
  avo_fail "agent changed branch or HEAD unexpectedly"
fi

avo_capture_candidate

if ! score_json=$(avo_run_score); then
  avo_persist_candidate "$tick" error
  avo_restore_tree
  if [ -n "${AVO_CAND_REF:-}" ] && [ -n "${AVO_CAND_PATCH:-}" ]; then
    payload=$(avo_lua json-object "n:tick=$tick" "b:correct=false" "n:objective=0" \
      "s:note=score infra failure" "s:candidate=$AVO_CAND_REF" "s:patch=$AVO_CAND_PATCH")
  else
    payload=$(avo_lua json-object "n:tick=$tick" "b:correct=false" "n:objective=0" "s:note=score infra failure")
  fi
  avo_ledger_append error "$payload"
  avo_maybe_supervise
  avo_fail "score command failed"
fi

correct=$(avo_json_get "$score_json" correct)
objective=$(avo_json_get "$score_json" objective float)
note=$(avo_json_get "$score_json" note)
stddev=$(avo_json_get "$score_json" stddev float)
best=$(avo_best_objective)

if [ "$(avo_should_commit "$correct" "$objective" "$stddev" "$best")" = yes ]; then
  avo_commit_candidate "avo($AVO_TASK): tick $tick objective=$objective"
  commit=$(avo_git rev-parse --short HEAD)
  payload=$(avo_lua json-object "n:tick=$tick" "b:correct=$correct" "f:objective=$objective" \
    "s:note=$note" "s:commit=$commit" "s:candidate=$commit")
  avo_ledger_append accept "$payload"
  AVO_START_HEAD=$(avo_git rev-parse HEAD)
  AVO_START_BRANCH=$(avo_current_branch)
  avo_capture_pre_tick
else
  avo_persist_candidate "$tick" reject
  if [ -n "${AVO_CAND_REF:-}" ] && [ -n "${AVO_CAND_PATCH:-}" ]; then
    payload=$(avo_lua json-object "n:tick=$tick" "b:correct=$correct" "f:objective=$objective" \
      "s:note=$note" "s:candidate=$AVO_CAND_REF" "s:patch=$AVO_CAND_PATCH")
  else
    payload=$(avo_lua json-object "n:tick=$tick" "b:correct=$correct" "f:objective=$objective" "s:note=$note")
  fi
  avo_restore_tree
  avo_ledger_append reject "$payload"
fi

avo_maybe_supervise
