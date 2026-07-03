#!/usr/bin/env bash
set -euo pipefail

# Production-safe primary recovery loop for DetMir ActivityWatch.
#
# Scope:
# - Detects ActivityWatch API poisoned datastore lock.
# - Pauses dependent secondary jobs before restarting the primary AW service.
# - Restarts only activitywatch-server inside the configured Proxmox CT.
# - Writes structured incident evidence.
#
# Non-goals:
# - Does not delete SQLite, journal or lock files.
# - Does not enable DLP, Loki or Velociraptor.
# - Does not restart Windows collectors.

AW_URL="${DETMIR_AW_RECOVERY_URL:-http://10.10.10.13:5600}"
AW_CT_ID="${DETMIR_AW_RECOVERY_CT_ID:-203}"
AW_SERVICE="${DETMIR_AW_RECOVERY_SERVICE:-activitywatch-server}"
STATE_DIR="${DETMIR_AW_RECOVERY_STATE_DIR:-/var/lib/detmir-aw-primary-recovery}"
HTTP_TIMEOUT_SECONDS="${DETMIR_AW_RECOVERY_HTTP_TIMEOUT_SECONDS:-8}"
CONFIRM_ATTEMPTS="${DETMIR_AW_RECOVERY_CONFIRM_ATTEMPTS:-2}"
CONFIRM_SLEEP_SECONDS="${DETMIR_AW_RECOVERY_CONFIRM_SLEEP_SECONDS:-5}"
STARTUP_TIMEOUT_SECONDS="${DETMIR_AW_RECOVERY_STARTUP_TIMEOUT_SECONDS:-120}"
STARTUP_SLEEP_SECONDS="${DETMIR_AW_RECOVERY_STARTUP_SLEEP_SECONDS:-5}"
COOLDOWN_SECONDS="${DETMIR_AW_RECOVERY_COOLDOWN_SECONDS:-900}"
DRY_RUN="${DETMIR_AW_RECOVERY_DRY_RUN:-0}"

PVE_PAUSE_UNITS="${DETMIR_AW_RECOVERY_PVE_PAUSE_UNITS:-aw-workforce-ingest.timer aw-workforce-ingest.service}"
PVE_RESUME_UNITS="${DETMIR_AW_RECOVERY_PVE_RESUME_UNITS:-aw-workforce-ingest.timer}"
CT_PAUSE_UNITS="${DETMIR_AW_RECOVERY_CT_PAUSE_UNITS:-aw-worktime-autoheal.timer aw-worktime-autoheal.service aw-worktime-prewarm.timer aw-worktime-prewarm.service aw-worktime-ui-bridge.timer aw-worktime-ui-bridge.service aw-worktime-influx-exporter.timer aw-worktime-influx-exporter.service aw-rus-healthd.timer aw-rus-healthd.service aw-slo-monitor.timer aw-slo-monitor.service}"
CT_RESUME_UNITS="${DETMIR_AW_RECOVERY_CT_RESUME_UNITS:-aw-worktime-autoheal.timer aw-worktime-prewarm.timer aw-worktime-ui-bridge.timer aw-worktime-influx-exporter.timer aw-rus-healthd.timer aw-slo-monitor.timer}"

usage() {
  cat <<'EOF'
Usage:
  detmir-aw-primary-recovery.sh [--once|--check-only|--self-test]

Environment:
  DETMIR_AW_RECOVERY_URL                  default: http://10.10.10.13:5600
  DETMIR_AW_RECOVERY_CT_ID                default: 203
  DETMIR_AW_RECOVERY_SERVICE              default: activitywatch-server
  DETMIR_AW_RECOVERY_STATE_DIR            default: /var/lib/detmir-aw-primary-recovery
  DETMIR_AW_RECOVERY_CONFIRM_ATTEMPTS     default: 2
  DETMIR_AW_RECOVERY_COOLDOWN_SECONDS     default: 900
  DETMIR_AW_RECOVERY_DRY_RUN              default: 0
  DETMIR_AW_RECOVERY_PVE_PAUSE_UNITS      space-separated units on Proxmox host
  DETMIR_AW_RECOVERY_PVE_RESUME_UNITS     space-separated units on Proxmox host
  DETMIR_AW_RECOVERY_CT_PAUSE_UNITS       space-separated units inside AW CT
  DETMIR_AW_RECOVERY_CT_RESUME_UNITS      space-separated units inside AW CT

The script never removes ActivityWatch SQLite, lock or journal files.
EOF
}

log() {
  printf 'ts=%s component=detmir-aw-primary-recovery %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    log "level=error event=missing_command command=$cmd"
    exit 127
  fi
}

json_string_array() {
  jq -Rsc 'split(" ") | map(select(length > 0))' <<<"$1"
}

record_incident() {
  local outcome="$1"
  local reason="$2"
  local recovered="$3"
  local started_at="$4"
  local finished_at="$5"
  local details="$6"
  local incident_id
  incident_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
  local incident_path="${STATE_DIR}/incidents/${incident_id}.json"
  install -d -m 0750 "${STATE_DIR}/incidents"
  jq -n \
    --arg generated_at_utc "$finished_at" \
    --arg started_at_utc "$started_at" \
    --arg outcome "$outcome" \
    --arg reason "$reason" \
    --arg recovered "$recovered" \
    --arg aw_url "$AW_URL" \
    --arg aw_ct_id "$AW_CT_ID" \
    --arg aw_service "$AW_SERVICE" \
    --arg details "$details" \
    --argjson pve_pause_units "$(json_string_array "$PVE_PAUSE_UNITS")" \
    --argjson ct_pause_units "$(json_string_array "$CT_PAUSE_UNITS")" \
    '{
      generated_at_utc: $generated_at_utc,
      started_at_utc: $started_at_utc,
      outcome: $outcome,
      reason: $reason,
      recovered: ($recovered == "true"),
      aw_url: $aw_url,
      aw_ct_id: ($aw_ct_id | tonumber? // $aw_ct_id),
      aw_service: $aw_service,
      details: $details,
      pve_pause_units: $pve_pause_units,
      ct_pause_units: $ct_pause_units
    }' >"$incident_path"
  ln -sfn "$incident_path" "${STATE_DIR}/latest.json"
  log "level=info event=incident_written path=$incident_path outcome=$outcome reason=$reason recovered=$recovered"
}

classify_http_response() {
  local code="$1"
  local body_file="$2"
  if [[ "$code" =~ ^2[0-9][0-9]$ ]]; then
    printf 'ok'
    return 0
  fi
  if grep -Eiq 'poisoned lock|Taking datastore lock failed|datastore lock failed' "$body_file"; then
    printf 'poisoned_lock'
    return 0
  fi
  printf 'http_%s' "$code"
}

probe_path() {
  local path="$1"
  local body_file
  body_file="$(mktemp)"
  local code rc
  code="$(curl -sS --max-time "$HTTP_TIMEOUT_SECONDS" -o "$body_file" -w '%{http_code}' "${AW_URL}${path}" 2>/dev/null)" || rc=$?
  rc="${rc:-0}"
  if [[ "$rc" -ne 0 ]]; then
    rm -f "$body_file"
    printf 'timeout'
    return 0
  fi
  local status
  status="$(classify_http_response "$code" "$body_file")"
  rm -f "$body_file"
  printf '%s' "$status"
}

ct_systemctl() {
  pct exec "$AW_CT_ID" -- systemctl "$@"
}

activitywatch_service_status() {
  if ! pct status "$AW_CT_ID" >/dev/null 2>&1; then
    printf 'ct_unavailable'
    return 0
  fi
  if ct_systemctl is-active --quiet "$AW_SERVICE"; then
    printf 'active'
  else
    printf 'inactive'
  fi
}

probe_aw() {
  local service_state settings_status buckets_status
  service_state="$(activitywatch_service_status)"
  if [[ "$service_state" != "active" ]]; then
    printf 'service_%s' "$service_state"
    return 0
  fi
  settings_status="$(probe_path '/api/0/settings/')"
  buckets_status="$(probe_path '/api/0/buckets/')"
  if [[ "$settings_status" == "ok" && "$buckets_status" == "ok" ]]; then
    printf 'ok'
  elif [[ "$settings_status" == "poisoned_lock" || "$buckets_status" == "poisoned_lock" ]]; then
    printf 'poisoned_lock'
  else
    printf '%s,%s' "$settings_status" "$buckets_status"
  fi
}

stop_units_on_host() {
  local units="$1"
  local unit
  for unit in $units; do
    log "level=info event=stop_unit scope=pve unit=$unit"
    systemctl stop "$unit" >/dev/null 2>&1 || true
  done
}

start_units_on_host() {
  local units="$1"
  local unit
  for unit in $units; do
    log "level=info event=start_unit scope=pve unit=$unit"
    systemctl start "$unit" >/dev/null 2>&1 || true
  done
}

stop_units_in_ct() {
  local units="$1"
  local unit
  for unit in $units; do
    log "level=info event=stop_unit scope=ct ct=$AW_CT_ID unit=$unit"
    ct_systemctl stop "$unit" >/dev/null 2>&1 || true
  done
}

start_units_in_ct() {
  local units="$1"
  local unit
  for unit in $units; do
    log "level=info event=start_unit scope=ct ct=$AW_CT_ID unit=$unit"
    ct_systemctl start "$unit" >/dev/null 2>&1 || true
  done
}

cooldown_active() {
  local last_file="${STATE_DIR}/last_restart_epoch"
  [[ -f "$last_file" ]] || return 1
  local now last
  now="$(date -u +%s)"
  last="$(cat "$last_file" 2>/dev/null || printf '0')"
  [[ "$last" =~ ^[0-9]+$ ]] || return 1
  (( now - last < COOLDOWN_SECONDS ))
}

wait_for_aw_ok() {
  local deadline now status
  deadline=$(( $(date -u +%s) + STARTUP_TIMEOUT_SECONDS ))
  while true; do
    status="$(probe_aw)"
    if [[ "$status" == "ok" ]]; then
      return 0
    fi
    now="$(date -u +%s)"
    if (( now >= deadline )); then
      log "level=error event=wait_for_aw_timeout last_status=$status"
      return 1
    fi
    log "level=info event=wait_for_aw status=$status"
    sleep "$STARTUP_SLEEP_SECONDS"
  done
}

run_recovery() {
  local reason="$1"
  local started_at="$2"
  if cooldown_active; then
    local finished_at
    finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    record_incident "skipped" "cooldown_active_after_${reason}" "false" "$started_at" "$finished_at" "restart suppressed by cooldown"
    return 0
  fi

  if [[ "$DRY_RUN" == "1" ]]; then
    local finished_at
    finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    record_incident "dry_run" "$reason" "false" "$started_at" "$finished_at" "dry run requested"
    return 0
  fi

  log "level=warn event=recovery_start reason=$reason ct=$AW_CT_ID service=$AW_SERVICE"
  stop_units_on_host "$PVE_PAUSE_UNITS"
  stop_units_in_ct "$CT_PAUSE_UNITS"

  log "level=warn event=restart_primary ct=$AW_CT_ID service=$AW_SERVICE"
  ct_systemctl restart "$AW_SERVICE"
  printf '%s\n' "$(date -u +%s)" >"${STATE_DIR}/last_restart_epoch"

  local finished_at
  if wait_for_aw_ok; then
    start_units_in_ct "$CT_RESUME_UNITS"
    start_units_on_host "$PVE_RESUME_UNITS"
    finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    record_incident "recovered" "$reason" "true" "$started_at" "$finished_at" "primary restarted and AW API returned ok"
    return 0
  fi

  finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  record_incident "failed" "$reason" "false" "$started_at" "$finished_at" "primary restart did not restore AW API; secondary timers left paused"
  return 1
}

check_once() {
  install -d -m 0750 "$STATE_DIR"
  local started_at status attempt reason
  started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  reason=""
  for attempt in $(seq 1 "$CONFIRM_ATTEMPTS"); do
    status="$(probe_aw)"
    log "level=info event=probe attempt=$attempt status=$status"
    case "$status" in
      ok)
        jq -n \
          --arg generated_at_utc "$started_at" \
          --arg status "ok" \
          --arg aw_url "$AW_URL" \
          '{generated_at_utc: $generated_at_utc, status: $status, aw_url: $aw_url}' \
          >"${STATE_DIR}/status.json"
        return 0
        ;;
      poisoned_lock|service_inactive|service_ct_unavailable)
        reason="$status"
        ;;
      *)
        local finished_at
        finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        record_incident "observed_no_action" "$status" "false" "$started_at" "$finished_at" "unhealthy state is not in automatic recovery allowlist"
        return 0
        ;;
    esac
    if [[ "$attempt" -lt "$CONFIRM_ATTEMPTS" ]]; then
      sleep "$CONFIRM_SLEEP_SECONDS"
    fi
  done
  run_recovery "$reason" "$started_at"
}

self_test() {
  local tmp
  tmp="$(mktemp)"
  printf '{"message":"Taking datastore lock failed, returning 504: poisoned lock: another task failed inside"}' >"$tmp"
  [[ "$(classify_http_response 503 "$tmp")" == "poisoned_lock" ]]
  printf '{"ok":true}' >"$tmp"
  [[ "$(classify_http_response 200 "$tmp")" == "ok" ]]
  printf '{"message":"other"}' >"$tmp"
  [[ "$(classify_http_response 503 "$tmp")" == "http_503" ]]
  rm -f "$tmp"
  log "level=info event=self_test status=ok"
}

main() {
  local mode="${1:---once}"
  case "$mode" in
    --help|-h)
      usage
      ;;
    --self-test)
      require_command jq
      self_test
      ;;
    --check-only)
      require_command curl
      require_command jq
      require_command pct
      status="$(probe_aw)"
      log "level=info event=check_only status=$status"
      [[ "$status" == "ok" ]]
      ;;
    --once)
      require_command curl
      require_command jq
      require_command pct
      require_command flock
      install -d -m 0750 "$STATE_DIR"
      exec 9>"${STATE_DIR}/lock"
      if ! flock -n 9; then
        log "level=warn event=lock_busy"
        exit 0
      fi
      check_once
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
