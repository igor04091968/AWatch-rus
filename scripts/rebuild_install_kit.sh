#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KIT_DIR="install-kit-awindows-20260427-211240"
ZIP_ARCHIVE="${KIT_DIR}.zip"
TAR_ARCHIVE="${KIT_DIR}.tar.gz"
SERVER_CONFIG_DIR="${KIT_DIR}/server-configs-192.168.100.18"
OLD_SERVER_CONFIG_DIR="${KIT_DIR}/server-configs-192.168.100.21"
TMP_SERVER_CONFIG_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_SERVER_CONFIG_DIR"' EXIT

copy_file() {
  local src="$1"
  local dest="$2"
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
}

sync_tree() {
  local base="$1"
  shift
  for rel in "$@"; do
    copy_file "$rel" "${KIT_DIR}/${rel}"
  done
}

if [[ -d "$OLD_SERVER_CONFIG_DIR" ]]; then
  cp "$OLD_SERVER_CONFIG_DIR"/*.deployment-config.json "$TMP_SERVER_CONFIG_DIR"/
fi

rm -rf "${KIT_DIR}/ansible" "${KIT_DIR}/aw-server" "${KIT_DIR}/windows" "${KIT_DIR}/server-configs-"*

ansible_files=(
  ansible/README.md
  ansible/deploy_aw_pfsense_poller.yml
  ansible/deploy_aw_server.yml
  ansible/deploy_aw_windows.yml
  ansible/group_vars/all.example.yml
  ansible/group_vars/pfsense-poller.example.yml
  ansible/group_vars/proxmox-matrix.example.yml
  ansible/group_vars/proxmox.example.yml
  ansible/group_vars/windows.example.yml
  ansible/install_full_stack.yml
  ansible/inventory.example.ini
  ansible/provision_proxmox_ct_and_deploy_aw.yml
  ansible/provision_proxmox_ct_matrix_and_deploy_aw.yml
  ansible/tasks/provision_ct_and_deploy_aw.yml
)

aw_server_files=(
  aw-server/activitywatch-server.service
  aw-server/apply_webui_ru_patch.sh
  aw-server/aw-host-groups.json
  aw-server/aw-ru-patch.js
  aw-server/aw-rus-healthd.py
  aw-server/aw-rus-healthd.service
  aw-server/aw-rus-healthd.timer
  aw-server/aw-server.env.example
  aw-server/aw-sw-cleanup.js
  aw-server/aw-worktime-api.py
  aw-server/aw-worktime-api.service
  aw-server/aw-worktime-panel.js
  aw-server/install_aw_server.sh
  aw-server/settings/classes-worktime.json
  aw-server/settings/views-default.json
)

windows_files=(
  windows/ActivityWatch.Windows.Common.psd1
  windows/ActivityWatch.Windows.Common.psm1
  windows/browser-domains-native-collector.ps1
  windows/deploy-domain-users.ps1
  windows/deploy-ensemble.ps1
  windows/deploy-single-user.ps1
  windows/dlp-endpoint-signals-collector.ps1
  windows/dlp-policy.example.json
  windows/email-outbound-collector.ps1
  windows/hardening-recovery.ps1
  windows/migrate-awatch-rus-paths.ps1
  windows/validate-deployment.ps1
  windows/web-category-rules.example.json
  windows/worktime-session-collector.ps1
)

sync_tree ansible "${ansible_files[@]}"
sync_tree aw-server "${aw_server_files[@]}"
sync_tree windows "${windows_files[@]}"

mkdir -p "$SERVER_CONFIG_DIR"
if compgen -G "$TMP_SERVER_CONFIG_DIR/*.deployment-config.json" >/dev/null; then
  cp "$TMP_SERVER_CONFIG_DIR"/*.deployment-config.json "$SERVER_CONFIG_DIR"/
fi

cat > "${KIT_DIR}/README-INSTALL-KIT.txt" <<'EOF'
ActivityWatch DetMir Windows Install Kit

Includes:
- windows/* (deploy scripts, collectors, common module, configs/examples)
- ansible/* (Windows and AW server playbooks, examples, inventory, tasks)
- aw-server/* (server installer, health orchestrator, RU patch loader, host groups, default settings)
- server-configs-192.168.100.18/* (working Windows/RDP config snapshots)

Source:
- Local project snapshot at build time.
EOF

python3 - <<'PY'
from pathlib import Path
import hashlib

root = Path('install-kit-awindows-20260427-211240')
manifest = root / 'MANIFEST.txt'
files = sorted(p for p in root.rglob('*') if p.is_file() and p.name != 'MANIFEST.txt')
with manifest.open('w', encoding='utf-8') as handle:
    for path in files:
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        handle.write(f"{digest}  {path.as_posix()}\n")
PY

rm -f "$ZIP_ARCHIVE" "$TAR_ARCHIVE"
zip -rq "$ZIP_ARCHIVE" "$KIT_DIR"
tar -czf "$TAR_ARCHIVE" "$KIT_DIR"
