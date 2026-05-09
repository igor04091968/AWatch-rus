#!/usr/bin/env bash
set -euo pipefail

# WinRM uses requests/pywinrm which may pick up local proxy settings (systemd env, shells, etc).
# If that happens, WinRM traffic can be sent to 127.0.0.1:<proxy> and time out.
# This wrapper hard-disables proxy env vars to make deploy deterministic.

if [[ -z "${AW_WINRM_PASSWORD:-}" ]]; then
  echo "ERROR: AW_WINRM_PASSWORD is not set" >&2
  echo "Usage: AW_WINRM_PASSWORD='...' ./run_deploy_aw_windows.sh [ansible-playbook args...]" >&2
  exit 2
fi

unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY all_proxy ALL_PROXY

exec ansible-playbook -i inventory.ini deploy_aw_windows.yml "$@"

