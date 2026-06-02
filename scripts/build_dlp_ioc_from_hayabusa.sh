#!/usr/bin/env bash
set -euo pipefail

# Build IOC blacklist artifacts for DLP from hayabusa-rules (Sigma YAML).
#
# Defaults:
#   rules root: /mnt/usb_hdd1/Projects/hayabusa/rules
#   output dir: ./data/dlp-ioc
#
# Usage:
#   scripts/build_dlp_ioc_from_hayabusa.sh [RULES_ROOT] [OUT_DIR]

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RULES_ROOT="${1:-/mnt/usb_hdd1/Projects/hayabusa/rules}"
OUT_DIR="${2:-$REPO_ROOT/data/dlp-ioc}"
TARGET_ROOT="${CARGO_TARGET_DIR:-$REPO_ROOT/adk-rust/target}"
RUST_BIN="${EXTRACT_IOC_FROM_SIGMA_RUST:-}"

if [[ ! -d "$RULES_ROOT" ]]; then
  echo "ERROR: rules root not found: $RULES_ROOT" >&2
  exit 2
fi

mkdir -p "$OUT_DIR"

rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/extract-ioc-from-sigma"
  "$REPO_ROOT/adk-rust/target/release/extract-ioc-from-sigma"
  "/usr/local/bin/extract-ioc-from-sigma"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    "$candidate" \
      --rules-root "$RULES_ROOT" \
      --out-dir "$OUT_DIR"
    echo "IOC artifacts generated in: $OUT_DIR"
    exit 0
  fi
done

python3 "$REPO_ROOT/scripts/extract_ioc_from_sigma.py" \
  --rules-root "$RULES_ROOT" \
  --out-dir "$OUT_DIR"

echo "IOC artifacts generated in: $OUT_DIR"
