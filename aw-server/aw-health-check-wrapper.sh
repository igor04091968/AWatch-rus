#!/usr/bin/env bash
set -euo pipefail

if [[ -x /usr/local/bin/aw-health-check-rust ]]; then
  exec /usr/local/bin/aw-health-check-rust "$@"
fi

echo "ERROR: /usr/local/bin/aw-health-check-rust is missing or not executable" >&2
exit 127
