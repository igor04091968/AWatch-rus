#!/usr/bin/env bash
set -euo pipefail

if [[ -x /usr/local/bin/check-aw-data-rust ]]; then
  if [[ -z "${AW_CHECK_SERVER:-}" && -z "${AW_SERVER_URL:-}" ]]; then
    for arg in "$@"; do
      if [[ "$arg" == "--server" || "$arg" == --server=* ]]; then
        exec /usr/local/bin/check-aw-data-rust "$@"
      fi
    done
    exec /usr/local/bin/check-aw-data-rust --server http://127.0.0.1:5600 "$@"
  fi
  exec /usr/local/bin/check-aw-data-rust "$@"
fi

exec /opt/activitywatch/aw-rus-ops/check-aw-data.sh "$@"
