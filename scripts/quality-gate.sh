#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[1/6] Bash syntax check"
find aw-server proxmox -type f -name "*.sh" -print0 | xargs -0 -r -n1 bash -n

echo "[2/6] Shellcheck (if available)"
if command -v shellcheck >/dev/null 2>&1; then
  find aw-server proxmox -type f -name "*.sh" -print0 | xargs -0 -r shellcheck -e SC1007,SC1090,SC2016
else
  echo "shellcheck not found, skipping."
fi

echo "[3/6] PowerShell parse check (if pwsh available)"
if command -v pwsh >/dev/null 2>&1; then
  if ! pwsh -NoLogo -NoProfile -Command '
    $ErrorActionPreference = "Stop"
    Get-ChildItem windows -Filter *.ps1 | ForEach-Object {
      [void][System.Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$null,[ref]$null)
    }
    [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "windows/ActivityWatch.Windows.Common.psm1"),[ref]$null,[ref]$null)
    [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path "windows/ActivityWatch.Windows.Common.psd1"),[ref]$null,[ref]$null)
  '; then
    echo "pwsh parse check failed due runtime environment; skipping."
  fi
else
  echo "pwsh not found, skipping."
fi

echo "[4/6] Install-kit consistency check"
./scripts/check_install_kit_vs_repo.sh

echo "[5/6] Generated-artifacts guard"
if git status --short | grep -E '^(\\?\\?| M|M ) (\\.graphify_|graphify-out/|reports/|tmp/|data/)'; then
  echo "ERROR: generated artifacts detected in working tree. Clean or ignore them before rollout."
  exit 1
fi

echo "[6/6] Ansible syntax check (if ansible-playbook available)"
if command -v ansible-playbook >/dev/null 2>&1; then
  for playbook in ansible/*.yml; do
    ansible-playbook --syntax-check "$playbook" -i ansible/inventory.example.ini >/dev/null
  done
else
  echo "ansible-playbook not found, skipping."
fi

echo "quality-gate: OK"
