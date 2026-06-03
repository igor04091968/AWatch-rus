#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RUST_BIN="${REBUILD_INSTALL_KIT_RUST:-}"

rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/rebuild-install-kit"
  "$ROOT_DIR/adk-rust/target/release/rebuild-install-kit"
  "/usr/local/bin/rebuild-install-kit"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" --root "$ROOT_DIR" "$@"
  fi
done

echo "ERROR: Rust install-kit builder not found. Build it with: cd '$ROOT_DIR/adk-rust' && cargo build --release -p rebuild-install-kit" >&2
exit 127
