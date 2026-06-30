#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RUST_BIN="${DIAG_AND_MANUAL_RESTART_RUST:-}"
rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/diag-and-manual-restart"
  "$ROOT_DIR/adk-rust/target/release/diag-and-manual-restart"
  "/usr/local/bin/diag-and-manual-restart"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" "$@"
  fi
done

INVENTORY="${INVENTORY:-ansible/inventory.ini}"
WITH_WINDOWS=0
AUTO_YES=0

usage() {
  cat <<'EOF'
Usage:
  scripts/diag_and_manual_restart.sh [--with-windows] [--yes] [--inventory <path>]

Behavior:
  1) Runs remote diagnostics on aw_server using /usr/local/bin/aw-health-check and /usr/local/bin/dlp-health-check
  2) If diagnostics fail:
     - restarts required server services
     - optionally restarts Windows launch/recovery tasks (with --with-windows)
  3) Runs diagnostics again and reports final status
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --with-windows) WITH_WINDOWS=1; shift ;;
    --yes) AUTO_YES=1; shift ;;
    --inventory) INVENTORY="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

log() { printf "%s %s\n" "$(date +"%F %T")" "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

is_truthy() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

require_real_value() {
  local name="$1"
  local value="${!name:-}"
  if [[ -z "$value" ]]; then
    die "missing required variable: $name"
  fi
  case "$value" in
    *192.0.2.*|*198.51.100.*|*203.0.113.*|*HOST-EXAMPLE*|*.example*)
      die "refusing placeholder value for $name: $value"
      ;;
  esac
}

command -v ansible >/dev/null 2>&1 || die "ansible not found"
command -v ansible-playbook >/dev/null 2>&1 || die "ansible-playbook not found"
[[ -f "$INVENTORY" ]] || die "inventory not found: $INVENTORY"

run_health_check() {
  ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.command -a "/usr/local/bin/aw-health-check" &&
  ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.command -a "/usr/local/bin/dlp-health-check"
}

restart_server_components() {
  log "Restarting server components on aw_server..."
  local units=(
    "activitywatch-server"
    "aw-worktime-api"
    "aw-worktime-ui-bridge.timer"
  )
  if is_truthy "${DETMIR_DLP_ENABLED:-${AW_DLP_ENABLED:-false}}"; then
    units+=(
      "aw-dlp-policy-engine.service"
      "aw-dlp-aggregator.timer"
      "activitywatch-dlp-aggregator.timer"
    )
  fi
  for unit in "${units[@]}"; do
    if ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.command -a "systemctl status ${unit}" >/dev/null 2>&1; then
      ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.systemd -a "name=${unit} state=restarted enabled=true" || true
    fi
  done
}

seed_server_dlp_events() {
  if ! is_truthy "${ALLOW_DLP_SEED_EVENTS:-0}"; then
    log "Skipping DLP freshness seeding; set ALLOW_DLP_SEED_EVENTS=1 with real DETMIR_HOSTNAME/DETMIR_AW_SERVER_HOST to allow it."
    return 0
  fi
  require_real_value DETMIR_HOSTNAME
  require_real_value DETMIR_AW_SERVER_HOST
  log "Seeding DLP freshness events on aw_server..."
  local ts host server_host
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  host="${DETMIR_HOSTNAME}"
  server_host="${DETMIR_AW_SERVER_HOST}"
  ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.shell -a "cat >/tmp/aw-endpoint-seed.json <<'JSON'
{\"timestamp\":\"${ts}\",\"duration\":0.0,\"data\":{\"hostname\":\"${host}\",\"signalType\":\"self_test\",\"source\":\"diag_and_manual_restart\",\"username\":\"system\",\"queueDepth\":0,\"eventsEnqueued\":0,\"eventsFlushed\":0,\"sendFailures\":0}}
JSON
cat >/tmp/aw-fileops-seed-host.json <<'JSON'
{\"timestamp\":\"${ts}\",\"duration\":0.0,\"data\":{\"hostname\":\"${host}\",\"operation\":\"self_test\",\"source\":\"diag_and_manual_restart\"}}
JSON
cat >/tmp/aw-fileops-seed-server.json <<'JSON'
{\"timestamp\":\"${ts}\",\"duration\":0.0,\"data\":{\"hostname\":\"${server_host}\",\"operation\":\"self_test\",\"source\":\"diag_and_manual_restart\"}}
JSON
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-endpoint-signals_${host}' -H 'Content-Type: application/json' -d '{\"client\":\"aw-dlp-endpoint-signals\",\"type\":\"aw.dlp.endpoint.signal\",\"hostname\":\"${host}\"}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_${host}' -H 'Content-Type: application/json' -d '{\"client\":\"aw-file-operations\",\"type\":\"aw.file.operation\",\"hostname\":\"${host}\"}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_${server_host}' -H 'Content-Type: application/json' -d '{\"client\":\"aw-file-operations\",\"type\":\"aw.file.operation\",\"hostname\":\"${server_host}\"}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-endpoint-signals_${host}/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-endpoint-seed.json >/dev/null
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_${host}/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-fileops-seed-host.json >/dev/null
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_${server_host}/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-fileops-seed-server.json >/dev/null
" >/dev/null
}

restart_windows_collectors() {
  log "Restarting Windows recovery/launch tasks on aw_windows..."
  ansible -i "$INVENTORY" aw_windows -m ansible.windows.win_shell -a "powershell -NoProfile -ExecutionPolicy Bypass -Command \"\$ErrorActionPreference = 'Stop'; try { Start-ScheduledTask -TaskName 'ActivityWatch Recovery' -ErrorAction Stop | Out-Null } catch {}; Get-ScheduledTask | Where-Object TaskName -like 'ActivityWatch Launch *' | ForEach-Object { try { Start-ScheduledTask -TaskName \$_.TaskName -ErrorAction Stop | Out-Null } catch {} }; Write-Output 'windows-tasks-restarted'\""
}

seed_windows_dlp_events() {
  if ! is_truthy "${ALLOW_DLP_SEED_EVENTS:-0}"; then
    log "Skipping Windows DLP freshness seeding; set ALLOW_DLP_SEED_EVENTS=1 with real DETMIR_HOSTNAME/DETMIR_AW_API to allow it."
    return 0
  fi
  require_real_value DETMIR_HOSTNAME
  require_real_value DETMIR_AW_API
  log "Seeding endpoint/file-ops events from aw_windows..."
  ansible -i "$INVENTORY" aw_windows -m ansible.windows.win_shell -a "powershell -NoProfile -ExecutionPolicy Bypass -Command \"\$ErrorActionPreference = 'Stop'; \$ts = (Get-Date).ToUniversalTime().ToString('o'); \$api='${DETMIR_AW_API}'; \$hostName='${DETMIR_HOSTNAME}'; \$endpointBucket=\$api + '/buckets/aw-dlp-endpoint-signals_' + \$hostName; \$fileopsBucket=\$api + '/buckets/aw-file-operations_' + \$hostName; \$endpoint=@{timestamp=\$ts;duration=0.0;data=@{hostname=\$hostName;signalType='self_test';source='diag_and_manual_restart';username=\$env:USERNAME;queueDepth=0;eventsEnqueued=0;eventsFlushed=0;sendFailures=0}} | ConvertTo-Json -Depth 8 -Compress; \$fileops=@{timestamp=\$ts;duration=0.0;data=@{hostname=\$hostName;operation='self_test';source='diag_and_manual_restart';username=\$env:USERNAME}} | ConvertTo-Json -Depth 8 -Compress; Invoke-RestMethod -Method Post -Uri \$endpointBucket -ContentType 'application/json' -Body (@{client='aw-dlp-endpoint-signals';type='aw.dlp.endpoint.signal';hostname=\$hostName} | ConvertTo-Json -Compress) -TimeoutSec 15 -DisableKeepAlive -ErrorAction SilentlyContinue | Out-Null; Invoke-RestMethod -Method Post -Uri \$fileopsBucket -ContentType 'application/json' -Body (@{client='aw-file-operations';type='aw.file.operation';hostname=\$hostName} | ConvertTo-Json -Compress) -TimeoutSec 15 -DisableKeepAlive -ErrorAction SilentlyContinue | Out-Null; Invoke-RestMethod -Method Post -Uri (\$endpointBucket + '/heartbeat?pulsetime=30') -ContentType 'application/json' -Body \$endpoint -TimeoutSec 15 -DisableKeepAlive | Out-Null; Invoke-RestMethod -Method Post -Uri (\$fileopsBucket + '/heartbeat?pulsetime=30') -ContentType 'application/json' -Body \$fileops -TimeoutSec 15 -DisableKeepAlive | Out-Null; Write-Output 'windows-dlp-seeded'\""
}

confirm_restart() {
  if [[ "$AUTO_YES" -eq 1 ]]; then
    return 0
  fi
  read -r -p "Diagnostics failed. Restart required components now? [y/N]: " answer
  [[ "${answer:-}" =~ ^[Yy]$ ]]
}

log "Running diagnostics on aw_server..."
if run_health_check; then
  log "Diagnostics: healthy. Restart not needed."
  exit 0
fi

log "Diagnostics: FAILED."
if ! confirm_restart; then
  log "Restart declined."
  exit 1
fi

restart_server_components
if [[ "$WITH_WINDOWS" -eq 1 ]]; then
  restart_windows_collectors
  seed_windows_dlp_events
fi
seed_server_dlp_events

log "Waiting 15 seconds before re-check..."
sleep 15

log "Running post-restart diagnostics..."
if run_health_check; then
  log "Post-restart diagnostics: healthy."
  exit 0
fi

log "Post-restart diagnostics: still failing."
exit 1
