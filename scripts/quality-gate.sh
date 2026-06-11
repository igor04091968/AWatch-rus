#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RUST_BIN="${QUALITY_GATE_RUST:-}"

rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/quality-gate"
  "$ROOT_DIR/adk-rust/target/release/quality-gate"
  "/usr/local/bin/quality-gate"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" --root "$ROOT_DIR" "$@"
  fi
done

echo "[1/6] Bash syntax check"
find aw-server proxmox scripts -type f -name "*.sh" -print0 | xargs -0 -r -n1 bash -n

echo "[2/6] Shellcheck (if available)"
if command -v shellcheck >/dev/null 2>&1; then
  {
    find aw-server proxmox -type f -name "*.sh"
    printf '%s\n' scripts/aw-webui-browser-smoke.sh
  } | xargs -r shellcheck -e SC1007,SC1090,SC2016
else
  echo "shellcheck not found, skipping."
fi

echo "[3/6] Node syntax check (if node available)"
if command -v node >/dev/null 2>&1; then
  node --check scripts/aw-webui-browser-smoke.mjs >/dev/null
else
  echo "node not found, skipping."
fi

echo "[4/6] PowerShell parse check (if pwsh available)"
if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoLogo -NoProfile -Command '
    $ErrorActionPreference = "Stop"
    Get-ChildItem windows -Filter *.ps1 | ForEach-Object {
      [void][System.Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$null,[ref]$null)
    }
    [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "windows/ActivityWatch.Windows.Common.psm1"),[ref]$null,[ref]$null)
    [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "windows/ActivityWatch.Windows.Common.psd1"),[ref]$null,[ref]$null)
  '
  if [[ -f windows/aw-collector-guard.ps1 ]]; then
    pwsh -NoLogo -NoProfile -File windows/aw-collector-guard.ps1 -SelfTest >/dev/null
  else
    echo "windows/aw-collector-guard.ps1 absent; Rust collector guard is the primary runtime."
  fi
else
  echo "pwsh not found, skipping."
fi



echo "[5/6] Ansible syntax check (if ansible-playbook available)"
if command -v ansible-playbook >/dev/null 2>&1; then
  for playbook in ansible/*.yml; do
    ansible-playbook --syntax-check "$playbook" -i ansible/inventory.example.ini >/dev/null
  done
else
  echo "ansible-playbook not found, skipping."
fi

echo "[6/6] DetMir Python runtime retirement guard"
if command -v git >/dev/null 2>&1; then
  mapfile -t tracked_py < <(git ls-files '*.py')
else
  mapfile -t tracked_py < <(find aw-server proxmox scripts ansible -type f -name '*.py' 2>/dev/null)
fi
violations=()
for path in "${tracked_py[@]}"; do
  case "$path" in
    aw-server/dlp-content-analysis/*|clickhouse-1c/ai/*|clickhouse-1c/etl/*|detmir-mcp/main.py|grafana-1c/*|pfsense/*|proxmox/tsj_guardian_bot.py|proxmox/test_tsj_guardian_bot.py)
      continue
      ;;
  esac
  case "$path" in
    aw-server/*|proxmox/*|scripts/*|ansible/*)
      violations+=("$path")
      ;;
  esac
done
if (( ${#violations[@]} > 0 )); then
  printf 'Python runtime regression in Rust-retired DetMir paths:\\n' >&2
  printf '%s\\n' "${violations[@]}" >&2
  exit 1
fi

echo "quality-gate: OK"
