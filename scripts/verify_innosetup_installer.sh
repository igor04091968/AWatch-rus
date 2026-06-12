#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

INSTALLER="${1:-windows/installkit/innosetup/AWatch-rus-InstallKit.exe}"
WINEPREFIX_VERIFY="${WINEPREFIX_VERIFY:-/tmp/aw-inno-verify-wineprefix}"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RUST_BIN="${VERIFY_INNOSETUP_INSTALLER_RUST:-}"
INSTALL_DIR_WIN='C:\AWatchRusExtract'
INSTALL_DIR_UNIX="${WINEPREFIX_VERIFY}/drive_c/AWatchRusExtract"

rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/verify-innosetup-installer"
  "$ROOT_DIR/adk-rust/target/release/verify-innosetup-installer"
  "/usr/local/bin/verify-innosetup-installer"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" "$INSTALLER" --root "$ROOT_DIR" --wineprefix "$WINEPREFIX_VERIFY" "${@:2}"
  fi
done

if [[ ! -f "$INSTALLER" ]]; then
  echo "Installer not found: $INSTALLER" >&2
  exit 1
fi

if ! command -v wine >/dev/null 2>&1; then
  echo "wine not found" >&2
  exit 1
fi

rm -rf "$WINEPREFIX_VERIFY"
mkdir -p "$WINEPREFIX_VERIFY"
export WINEPREFIX="$WINEPREFIX_VERIFY"
export WINEDEBUG="${WINEDEBUG:--all}"

wineboot -u >/dev/null 2>&1
wine "$INSTALLER" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /TASKS="" /DIR="$INSTALL_DIR_WIN" >/dev/null 2>&1
wineserver -w >/dev/null 2>&1

required_files=(
  windows/AWatchRusCollectorGuardService.cs
  windows/install-collector-guard-service.ps1
  windows/aw-windows-telemetry.exe
  windows/dlp-policy.native-cross-os.example.json
)

for rel in "${required_files[@]}"; do
  extracted="${INSTALL_DIR_UNIX}/${rel}"
  if [[ ! -f "$extracted" ]]; then
    echo "Missing extracted file: $rel" >&2
    exit 1
  fi
  repo_rel="$rel"
  if [[ "$rel" == "windows/aw-windows-telemetry.exe" ]]; then
    repo_rel="adk-rust/target/x86_64-pc-windows-gnu/release/aw-windows-telemetry.exe"
  fi
  if ! cmp -s "$repo_rel" "$extracted"; then
    echo "Extracted file differs from repo: $rel" >&2
    exit 1
  fi
done

echo "verify_innosetup_installer: OK"
