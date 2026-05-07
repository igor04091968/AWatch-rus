#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

timestamp() { date +"%Y%m%d-%H%M%S"; }

LOG_DIR="${ROOT_DIR}/.rollout-logs/$(timestamp)"
mkdir -p "$LOG_DIR"

log() { printf "%s %s\n" "$(date +"%F %T")" "$*" | tee -a "${LOG_DIR}/rollout.log" >&2; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { log "ERROR: missing command: $1"; exit 127; }
}

prompt_secret() {
  local var_name="$1"
  local prompt="$2"
  if [[ -n "${!var_name:-}" ]]; then
    return 0
  fi
  local _val
  read -r -s -p "${prompt}: " _val
  echo
  printf -v "$var_name" '%s' "$_val"
  declare -gx "$var_name"
}

require_cmd git
require_cmd ansible-playbook
require_cmd ansible

log "Repo: ${ROOT_DIR}"
log "Branch: $(git branch --show-current)"

if [[ "${AW_MAINTENANCE_ACK:-}" != "YES" ]]; then
  log "ERROR: maintenance window is required."
  log "Set AW_MAINTENANCE_ACK=YES to proceed."
  exit 4
fi

log "Running local quality gate..."
./scripts/quality-gate.sh | tee -a "${LOG_DIR}/quality-gate.log"

if [[ -f "${ROOT_DIR}/secrets/runtime.env" ]]; then
  log "Loading secrets/runtime.env"
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/secrets/runtime.env"
  set +a
fi

if [[ ! -f ansible/inventory.ini ]]; then
  log "ERROR: missing ansible/inventory.ini"
  log "Hint: copy ansible/inventory.example.ini -> ansible/inventory.ini and adjust hosts."
  exit 2
fi

if [[ -t 0 ]]; then
  prompt_secret AW_SSH_PASSWORD "Enter SSH password for aw_server (root@10.10.10.13)"
  prompt_secret AW_WINRM_PASSWORD "Enter WinRM password for aw_windows (192.168.100.21)"
fi

if [[ -z "${AW_SSH_PASSWORD:-}" || -z "${AW_WINRM_PASSWORD:-}" ]]; then
  log "ERROR: missing AW_SSH_PASSWORD or AW_WINRM_PASSWORD."
  log "Provide them via interactive prompt (TTY) or create secrets/runtime.env."
  exit 3
fi

log "Preflight connectivity..."
ansible -i ansible/inventory.ini aw_server -m ping | tee -a "${LOG_DIR}/ping_aw_server.log"
ansible -i ansible/inventory.ini aw_windows -m win_ping | tee -a "${LOG_DIR}/ping_aw_windows.log"

log "Preflight ActivityWatch API/data checks..."
./check-aw-data.sh | tee -a "${LOG_DIR}/check_aw_data.log"
./check-aw-full.sh | tee -a "${LOG_DIR}/check_aw_full.log"

log "Dry-run aw_server..."
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml --check --diff | tee -a "${LOG_DIR}/check_aw_server.log"

log "Deploy aw_server..."
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml | tee -a "${LOG_DIR}/deploy_aw_server.log"

log "Dry-run aw_windows..."
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_windows.yml --check --diff | tee -a "${LOG_DIR}/check_aw_windows.log"

log "Deploy aw_windows..."
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_windows.yml | tee -a "${LOG_DIR}/deploy_aw_windows.log"

log "Post-validate aw_windows..."
ansible-playbook -i ansible/inventory.ini ansible/post_validate_aw_windows.yml | tee -a "${LOG_DIR}/post_validate_aw_windows.log"

log "DONE. Logs: ${LOG_DIR}"
