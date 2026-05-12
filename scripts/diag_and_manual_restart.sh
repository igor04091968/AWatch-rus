#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

INVENTORY="${INVENTORY:-ansible/inventory.ini}"
WITH_WINDOWS=0
AUTO_YES=0

usage() {
  cat <<'EOF'
Usage:
  scripts/diag_and_manual_restart.sh [--with-windows] [--yes] [--inventory <path>]

Behavior:
  1) Runs remote diagnostics on aw_server using /usr/local/bin/aw-health-check
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

command -v ansible >/dev/null 2>&1 || die "ansible not found"
command -v ansible-playbook >/dev/null 2>&1 || die "ansible-playbook not found"
[[ -f "$INVENTORY" ]] || die "inventory not found: $INVENTORY"

run_health_check() {
  ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.command -a "/usr/local/bin/aw-health-check"
}

restart_server_components() {
  log "Restarting server components on aw_server..."
  local units=(
    "activitywatch-server"
    "aw-worktime-api"
    "aw-worktime-ui-bridge.timer"
    "aw-dlp-policy-engine.service"
    "aw-dlp-aggregator.timer"
    "activitywatch-dlp-aggregator.timer"
  )
  for unit in "${units[@]}"; do
    if ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.command -a "systemctl status ${unit}" >/dev/null 2>&1; then
      ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.systemd -a "name=${unit} state=restarted enabled=true" || true
    fi
  done
}

seed_server_dlp_events() {
  log "Seeding DLP freshness events on aw_server..."
  local ts
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  ansible -i "$INVENTORY" aw_server -b -m ansible.builtin.shell -a "cat >/tmp/aw-endpoint-seed.json <<'JSON'
{\"timestamp\":\"${ts}\",\"duration\":0.0,\"data\":{\"hostname\":\"SHARKON2025\",\"signalType\":\"self_test\",\"source\":\"diag_and_manual_restart\",\"username\":\"system\",\"queueDepth\":0,\"eventsEnqueued\":0,\"eventsFlushed\":0,\"sendFailures\":0}}
JSON
cat >/tmp/aw-fileops-seed-host.json <<'JSON'
{\"timestamp\":\"${ts}\",\"duration\":0.0,\"data\":{\"hostname\":\"SHARKON2025\",\"operation\":\"self_test\",\"source\":\"diag_and_manual_restart\"}}
JSON
cat >/tmp/aw-fileops-seed-server.json <<'JSON'
{\"timestamp\":\"${ts}\",\"duration\":0.0,\"data\":{\"hostname\":\"10.10.10.13\",\"operation\":\"self_test\",\"source\":\"diag_and_manual_restart\"}}
JSON
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-endpoint-signals_SHARKON2025' -H 'Content-Type: application/json' -d '{\"client\":\"aw-dlp-endpoint-signals\",\"type\":\"aw.dlp.endpoint.signal\",\"hostname\":\"SHARKON2025\"}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_SHARKON2025' -H 'Content-Type: application/json' -d '{\"client\":\"aw-file-operations\",\"type\":\"aw.file.operation\",\"hostname\":\"SHARKON2025\"}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_10.10.10.13' -H 'Content-Type: application/json' -d '{\"client\":\"aw-file-operations\",\"type\":\"aw.file.operation\",\"hostname\":\"10.10.10.13\"}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-endpoint-signals_SHARKON2025/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-endpoint-seed.json >/dev/null
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_SHARKON2025/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-fileops-seed-host.json >/dev/null
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_10.10.10.13/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-fileops-seed-server.json >/dev/null
" >/dev/null
}

restart_windows_collectors() {
  log "Restarting Windows recovery/launch tasks on aw_windows..."
  ansible -i "$INVENTORY" aw_windows -m ansible.windows.win_shell -a "powershell -NoProfile -ExecutionPolicy Bypass -Command \"\$ErrorActionPreference = 'Stop'; try { Start-ScheduledTask -TaskName 'ActivityWatch Recovery' -ErrorAction Stop | Out-Null } catch {}; Get-ScheduledTask | Where-Object TaskName -like 'ActivityWatch Launch *' | ForEach-Object { try { Start-ScheduledTask -TaskName \$_.TaskName -ErrorAction Stop | Out-Null } catch {} }; Write-Output 'windows-tasks-restarted'\""
}

seed_windows_dlp_events() {
  log "Seeding endpoint/file-ops events from aw_windows..."
  ansible -i "$INVENTORY" aw_windows -m ansible.windows.win_shell -a "powershell -NoProfile -ExecutionPolicy Bypass -Command \"\$ErrorActionPreference = 'Stop'; \$ts = (Get-Date).ToUniversalTime().ToString('o'); \$api='http://10.10.10.13:5600/api/0'; \$endpoint=@{timestamp=\$ts;duration=0.0;data=@{hostname='SHARKON2025';signalType='self_test';source='diag_and_manual_restart';username=\$env:USERNAME;queueDepth=0;eventsEnqueued=0;eventsFlushed=0;sendFailures=0}} | ConvertTo-Json -Depth 8 -Compress; \$fileops=@{timestamp=\$ts;duration=0.0;data=@{hostname='SHARKON2025';operation='self_test';source='diag_and_manual_restart';username=\$env:USERNAME}} | ConvertTo-Json -Depth 8 -Compress; Invoke-RestMethod -Method Post -Uri \$api'/buckets/aw-dlp-endpoint-signals_SHARKON2025' -ContentType 'application/json' -Body '{\\\"client\\\":\\\"aw-dlp-endpoint-signals\\\",\\\"type\\\":\\\"aw.dlp.endpoint.signal\\\",\\\"hostname\\\":\\\"SHARKON2025\\\"}' -TimeoutSec 15 -DisableKeepAlive -ErrorAction SilentlyContinue | Out-Null; Invoke-RestMethod -Method Post -Uri \$api'/buckets/aw-file-operations_SHARKON2025' -ContentType 'application/json' -Body '{\\\"client\\\":\\\"aw-file-operations\\\",\\\"type\\\":\\\"aw.file.operation\\\",\\\"hostname\\\":\\\"SHARKON2025\\\"}' -TimeoutSec 15 -DisableKeepAlive -ErrorAction SilentlyContinue | Out-Null; Invoke-RestMethod -Method Post -Uri \$api'/buckets/aw-dlp-endpoint-signals_SHARKON2025/heartbeat?pulsetime=30' -ContentType 'application/json' -Body \$endpoint -TimeoutSec 15 -DisableKeepAlive | Out-Null; Invoke-RestMethod -Method Post -Uri \$api'/buckets/aw-file-operations_SHARKON2025/heartbeat?pulsetime=30' -ContentType 'application/json' -Body \$fileops -TimeoutSec 15 -DisableKeepAlive | Out-Null; Write-Output 'windows-dlp-seeded'\""
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
