#!/usr/bin/env bash
set -euo pipefail

if [[ -x /usr/local/bin/aw-health-check-rust ]]; then
  exec /usr/local/bin/aw-health-check-rust "$@"
fi

exec /opt/activitywatch/aw-rus-ops/health-check.sh "$@"
