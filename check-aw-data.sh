#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"

for candidate in \
  "$TARGET_ROOT/release/check-aw-data" \
  "$TARGET_ROOT/debug/check-aw-data" \
  "$ROOT_DIR/adk-rust/target/release/check-aw-data" \
  "$ROOT_DIR/adk-rust/target/debug/check-aw-data"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" "$@"
  fi
done

exec "$ROOT_DIR/scripts/legacy/check-aw-data.sh" "$@"
