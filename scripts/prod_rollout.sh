#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ -f "${ROOT_DIR}/private-config/runtime.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/private-config/runtime.env"
  set +a
fi

if [[ "${1:-}" == "--apply-legacy" ]]; then
  shift
else
  TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
  for candidate in \
    "${PROD_ROLLOUT_RUST:-}" \
    "$TARGET_ROOT/release/prod-rollout" \
    "$ROOT_DIR/adk-rust/target/release/prod-rollout" \
    "/usr/local/bin/prod-rollout"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
      exec "$candidate" --root "$ROOT_DIR" "$@"
    fi
  done
  cat >&2 <<'EOF'
prod_rollout.sh now requires the Rust planner/orchestrator for safe default runs.
Build it first:
  cd adk-rust && cargo build --release -p prod-rollout

Safe checks:
  scripts/prod_rollout.sh --check-inputs --json

Explicit Rust rollout:
  scripts/prod_rollout.sh --apply

Old Bash rollout:
  scripts/prod_rollout.sh --apply-legacy
EOF
  exit 2
fi

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
  read -r -s -p "${prompt}: " "$var_name"
  echo
  export "$var_name"
}

require_cmd git
require_cmd ansible-playbook
require_cmd ansible

log "Repo: ${ROOT_DIR}"
log "Branch: $(git branch --show-current)"

log "Running local quality gate..."
./scripts/quality-gate.sh | tee -a "${LOG_DIR}/quality-gate.log"

if [[ -f "${ROOT_DIR}/private-config/runtime.env" ]]; then
  log "Loading private-config/runtime.env"
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/private-config/runtime.env"
  set +a
fi

if [[ ! -f ansible/inventory.ini ]]; then
  log "ERROR: missing ansible/inventory.ini"
  log "Hint: copy ansible/inventory.example.ini -> ansible/inventory.ini and adjust hosts."
  exit 2
fi

if [[ -t 0 ]]; then
  prompt_secret AW_SSH_PASSWORD "Enter SSH password for aw_server (root@192.0.2.13)"
  prompt_secret AW_WINRM_PASSWORD "Enter WinRM password for aw_windows (198.51.100.18)"
fi

if [[ -z "${AW_SSH_PASSWORD:-}" || -z "${AW_WINRM_PASSWORD:-}" ]]; then
  log "ERROR: missing AW_SSH_PASSWORD or AW_WINRM_PASSWORD."
  log "Provide them via interactive prompt (TTY) or create private-config/runtime.env."
  exit 3
fi

log "Preflight connectivity..."
ansible -i ansible/inventory.ini aw_server -m ping | tee -a "${LOG_DIR}/ping_aw_server.log"
ansible -i ansible/inventory.ini aw_windows -m win_ping | tee -a "${LOG_DIR}/ping_aw_windows.log"

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
