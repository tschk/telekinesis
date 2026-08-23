#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"

AVO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || avo_die "usage: status.sh (run from the task git repo)"
AVO_TASK=${1:-${AVO_TASK:-}}
avo_load_config

python3 - "$AVO_TASK" "$(avo_current_branch)" "$(avo_ledger_path)" "${AVO_STALL_AFTER:-3}" <<'PY'
import json, sys
task, branch, path, stall_after = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
try:
    rows = [json.loads(line) for line in open(path) if line.strip()]
except FileNotFoundError:
    rows = []
scored = [row for row in rows if row.get("kind") in ("accept", "reject", "error")]
best = None
best_tick = None
since_best = 0
for row in rows:
    if row.get("kind") == "baseline":
        obj = float(row.get("objective", 0))
        if best is None or obj > best:
            best = obj
            best_tick = row.get("tick", 0)
for row in scored:
    obj = float(row.get("objective", 0))
    if row.get("kind") == "accept" and (best is None or obj > best):
        best = obj
        best_tick = row.get("tick")
        since_best = 0
    else:
        since_best += 1
print(f"task: {task}")
print(f"branch: {branch}")
print(f"ticks: {len(scored)}")
if best is None:
    print("best: none")
else:
    print(f"best: {best} (tick {best_tick})")
print(f"stall: {since_best}/{stall_after}")
print("lineage:")
for row in scored[-8:]:
    print(
        f"  {row.get('tick')} {row.get('kind')} objective={row.get('objective')} "
        f"correct={row.get('correct')} note={row.get('note')}"
    )
PY
