#!/usr/bin/env bash
set -euo pipefail

BASE_DIR="/opt/activitywatch/dlp-content-analysis"
RUST_ANALYZER="/usr/local/bin/aw-dlp-content-analyzer-rust"
VENV_PY="$BASE_DIR/.venv/bin/python"
ANALYZER="$BASE_DIR/content_analyzer.py"

if [ -x "$RUST_ANALYZER" ]; then
  exec "$RUST_ANALYZER" "$@"
fi

if [ ! -x "$VENV_PY" ]; then
  echo "ERROR: content-analysis virtualenv is missing: $VENV_PY" >&2
  exit 1
fi

exec "$VENV_PY" "$ANALYZER" "$@"
