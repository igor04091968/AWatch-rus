#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

rust_candidates=(
  "${AW_BROWSER_SMOKE_RUST:-}"
  "${CARGO_TARGET_DIR:-}/release/aw-browser-smoke"
  "$ROOT_DIR/adk-rust/target/release/aw-browser-smoke"
  "/usr/local/bin/aw-browser-smoke"
)

for rust_bin in "${rust_candidates[@]}"; do
  if [[ -n "$rust_bin" && -x "$rust_bin" ]]; then
    exec "$rust_bin" --root "$ROOT_DIR" -- "$@"
  fi
done

if [[ -z "${NODE_PATH:-}" && -d "$HOME/.agents/skills/playwright/node_modules" ]]; then
  export NODE_PATH="$HOME/.agents/skills/playwright/node_modules"
fi

exec node "$ROOT_DIR/scripts/aw-webui-browser-smoke.mjs" "$@"
