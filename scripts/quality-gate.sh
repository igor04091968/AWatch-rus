#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[1/3] Bash syntax check"
find aw-server proxmox -type f -name "*.sh" -print0 | xargs -0 -r -n1 bash -n

echo "[2/3] Shellcheck (if available)"
if command -v shellcheck >/dev/null 2>&1; then
  find aw-server proxmox -type f -name "*.sh" -print0 | xargs -0 -r shellcheck
else
  echo "shellcheck not found, skipping."
fi

echo "[3/3] PowerShell parse check (if pwsh available)"
if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoLogo -NoProfile -Command '
    $ErrorActionPreference = "Stop"
    Get-ChildItem windows -Filter *.ps1 | ForEach-Object {
      [void][System.Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$null,[ref]$null)
    }
    [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "windows/ActivityWatch.Windows.Common.psm1"),[ref]$null,[ref]$null)
    [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "windows/ActivityWatch.Windows.Common.psd1"),[ref]$null,[ref]$null)
  '
else
  echo "pwsh not found, skipping."
fi

echo "quality-gate: OK"
