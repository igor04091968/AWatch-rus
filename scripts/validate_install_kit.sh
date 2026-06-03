#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KIT_DIR="install-kit-awindows-20260427-211240"
MANIFEST="$KIT_DIR/MANIFEST.txt"
ZIP_ARCHIVE="install-kit-awindows-20260427-211240.zip"
TAR_ARCHIVE="install-kit-awindows-20260427-211240.tar.gz"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RUST_BIN="${VALIDATE_INSTALL_KIT_RUST:-}"

rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/validate-install-kit"
  "$ROOT_DIR/adk-rust/target/release/validate-install-kit"
  "/usr/local/bin/validate-install-kit"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" \
      --root "$ROOT_DIR" \
      --kit-dir "$KIT_DIR" \
      --zip-archive "$ZIP_ARCHIVE" \
      --tar-archive "$TAR_ARCHIVE" \
      "$@"
  fi
done

echo "ERROR: Rust validator not found. Build it with: cd '$ROOT_DIR/adk-rust' && cargo build --release -p validate-install-kit" >&2
exit 127
