#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ -f "${ROOT_DIR}/secrets/runtime.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/secrets/runtime.env"
  set +a
fi

: "${AW_SSH_PASSWORD:?AW_SSH_PASSWORD is required}"
: "${AW_WINRM_PASSWORD:?AW_WINRM_PASSWORD is required}"

command -v sshpass >/dev/null 2>&1 || { echo "missing sshpass" >&2; exit 127; }
command -v ansible-playbook >/dev/null 2>&1 || { echo "missing ansible-playbook" >&2; exit 127; }

SERVER_HOST="${AW_SERVER_HOST:-10.10.10.13}"
SERVER_USER="${AW_SERVER_USER:-igor}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
REMOTE_BACKUP_DIR="/var/lib/activitywatch/backups/prod-restore-${TIMESTAMP}"
LEGACY_DB="/root/.local/share/activitywatch/aw-server-rust/sqlite.db"
TARGET_DB="/var/lib/activitywatch/.local/share/activitywatch/aw-server-rust/sqlite.db"
REMOTE_MERGE_SCRIPT="/tmp/merge_aw_server_dbs.py"

ssh_remote() {
  sshpass -p "$AW_SSH_PASSWORD" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "${SERVER_USER}@${SERVER_HOST}" "$@"
}

scp_remote() {
  sshpass -p "$AW_SSH_PASSWORD" scp -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "$@"
}

scp_remote "${ROOT_DIR}/scripts/merge_aw_server_dbs.py" "${SERVER_USER}@${SERVER_HOST}:${REMOTE_MERGE_SCRIPT}"

ssh_remote "sudo mkdir -p '${REMOTE_BACKUP_DIR}' && sudo chown root:root '${REMOTE_BACKUP_DIR}'"
ssh_remote "sudo test -f '${LEGACY_DB}'"
ssh_remote "sudo test -f '${TARGET_DB}'"
ssh_remote "sudo cp -a '${LEGACY_DB}' '${REMOTE_BACKUP_DIR}/legacy-root-sqlite.db' && sudo cp -a '${TARGET_DB}' '${REMOTE_BACKUP_DIR}/target-before-merge-sqlite.db'"
ssh_remote "sudo systemctl stop activitywatch-server.service || true"
ssh_remote "sudo python3 '${REMOTE_MERGE_SCRIPT}' --base '${LEGACY_DB}' --overlay '${TARGET_DB}' --output '${REMOTE_BACKUP_DIR}/sqlite.merged.db'"
ssh_remote "sudo install -o activitywatch -g activitywatch -m 0644 '${REMOTE_BACKUP_DIR}/sqlite.merged.db' '${TARGET_DB}'"

ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_windows.yml
ansible-playbook -i ansible/inventory.ini ansible/post_validate_aw_windows.yml

python3 - <<'PY'
import json, urllib.request
base = 'http://10.10.10.13:5600'
window_payload = {
    'timeperiods': ['2026-04-29T00:00:00+03:00/2026-04-29T23:59:59+03:00'],
    'query': [
        'window_events = query_bucket(find_bucket("aw-watcher-window_SHARKON2025"));',
        'RETURN = window_events;'
    ]
}
req = urllib.request.Request(base + '/api/0/query/', data=json.dumps(window_payload).encode(), method='POST', headers={'Content-Type': 'application/json', 'Origin': 'http://10.10.10.13:5600'})
with urllib.request.urlopen(req) as response:
    data = json.loads(response.read().decode())
window_count = len(data[0]) if isinstance(data, list) and data else 0
if window_count <= 0:
    raise SystemExit('no historical window data restored for 2026-04-29')
with urllib.request.urlopen(base + '/api/0/settings/') as response:
    settings = json.loads(response.read().decode())
if settings.get('always_active_pattern') != 'aw-watcher-window':
    raise SystemExit('always_active_pattern is not configured')
print(json.dumps({'restored_window_events_2026_04_29': window_count, 'always_active_pattern': settings.get('always_active_pattern')}, ensure_ascii=False))
PY
