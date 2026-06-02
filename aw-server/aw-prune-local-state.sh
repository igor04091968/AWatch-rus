#!/usr/bin/env bash
set -euo pipefail

if [[ -x /usr/local/bin/aw-prune-local-state-rust ]]; then
  if [[ $# -eq 0 ]]; then
    exec /usr/local/bin/aw-prune-local-state-rust --apply
  fi
  exec /usr/local/bin/aw-prune-local-state-rust "$@"
fi

exec /opt/activitywatch/aw-rus-ops/aw-prune-local-state.sh "$@"
