#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -z "${NODE_PATH:-}" && -d /home/igor/.agents/skills/playwright/node_modules ]]; then
  export NODE_PATH=/home/igor/.agents/skills/playwright/node_modules
fi

exec node "$ROOT_DIR/scripts/aw-webui-browser-smoke.mjs" "$@"
