#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KIT_DIR="install-kit-awindows-20260427-211240"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RUST_BIN="${CHECK_INSTALL_KIT_VS_REPO_RUST:-}"

rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/check-install-kit-vs-repo"
  "$ROOT_DIR/adk-rust/target/release/check-install-kit-vs-repo"
  "/usr/local/bin/check-install-kit-vs-repo"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" --root "$ROOT_DIR" --kit-dir "$KIT_DIR" "$@"
  fi
done

echo "ERROR: Rust checker not found. Build it with: cd '$ROOT_DIR/adk-rust' && cargo build --release -p check-install-kit-vs-repo" >&2
exit 127
