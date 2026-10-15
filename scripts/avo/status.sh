#!/usr/bin/env bash
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=lib.sh
. "$here/lib.sh"

AVO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || avo_die "usage: status.sh (run from the task git repo)"
AVO_TASK=${1:-${AVO_TASK:-}}
avo_load_config

avo_lua status "$AVO_TASK" "$(avo_current_branch)" "$(avo_ledger_path)" "${AVO_STALL_AFTER:-3}"
