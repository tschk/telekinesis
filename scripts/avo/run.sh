#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
max_ticks=${AVO_MAX_TICKS:-8}
args=()
init=false

usage() {
  cat >&2 <<EOF
usage: run.sh [--max-ticks <n>] [--init <task>] [--goal <text>] [--score <cmd>] [--agent <cmd>]
bounded AVO ticks; each tick is Vary(P_t, K, f) = Agent(...)
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --max-ticks)
      shift
      [ "$#" -gt 0 ] || { usage; exit 2; }
      max_ticks=$1
      ;;
    --init)
      init=true
      args+=("$1")
      ;;
    --help|-h)
      usage
      exit 2
      ;;
    *)
      args+=("$1")
      ;;
  esac
  shift
done

if [ "$init" = true ]; then
  "$here/tick.sh" "${args[@]}"
  args=()
fi

i=1
while [ "$i" -le "$max_ticks" ]; do
  if [ "${#args[@]}" -gt 0 ]; then
    "$here/tick.sh" "${args[@]}"
  else
    "$here/tick.sh"
  fi
  i=$((i + 1))
done
