#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-status}"
PROFILE="${2:-${AW_DLP_PROFILE:-core_only}}"
AW_BASE="${AW_DLP_CONTROL_AW_BASE:-http://127.0.0.1:5600}"
HOSTNAME_FILTER="${AW_DLP_CONTROL_HOSTNAME:-${AW_LOGICAL_HOST_ID:-${AW_MONITORED_WINDOWS_HOSTNAME:-HOST-EXAMPLE}}}"
STATE_DIR="${AW_DLP_CONTROL_STATE_DIR:-/var/lib/activitywatch/health}"
STATE_FILE="${AW_DLP_CONTROL_STATE_FILE:-${STATE_DIR}/dlp-runtime-state.json}"
STATE_HISTORY_DIR="${AW_DLP_CONTROL_HISTORY_DIR:-${STATE_DIR}/dlp-runtime-history}"
ROLLBACK_FILE="${AW_DLP_CONTROL_ROLLBACK_FILE:-${STATE_DIR}/dlp-runtime-rollback.state}"
REASON="${AW_DLP_DISABLED_REASON:-dlp_runtime_profile_control}"

DLP_UNITS=(
  aw-dlp-influx-exporter.timer
  aw-dlp-influx-exporter.service
  activitywatch-dlp-aggregator.timer
  activitywatch-dlp-aggregator.service
  aw-dlp-report-scheduler.timer
  aw-dlp-report-scheduler.service
  aw-dlp-syslog-forwarder.timer
  aw-dlp-syslog-forwarder.service
  aw-dlp-webhook-sender.timer
  aw-dlp-webhook-sender.service
  aw-dlp-cef-exporter.timer
  aw-dlp-cef-exporter.service
  aw-dlp-ioc-refresh.timer
  aw-dlp-ioc-refresh.service
  aw-dlp-policy-engine.service
  aw-dlp-case-management.service
  detmir-portal-evidence.service
)

DLP_BUCKET_PREFIXES=(
  aw-dlp-endpoint-signals
  aw-dlp-incidents
  aw-dlp-review
  aw-dlp-rules
)

DLP_LIGHT_UNITS=(
  activitywatch-dlp-aggregator.timer
  aw-dlp-ioc-refresh.timer
)

DLP_ON_DEMAND_UNITS=(
  aw-dlp-ioc-refresh.timer
  aw-dlp-policy-engine.service
  aw-dlp-case-management.service
  detmir-portal-evidence.service
)

json_escape() {
  local value="$1"
  python3 -c 'import json,sys; print(json.dumps(sys.argv[1], ensure_ascii=False))' "$value"
}

unit_json() {
  local first=1 unit active enabled load
  printf '['
  for unit in "${DLP_UNITS[@]}"; do
    load="$(systemctl show -p LoadState --value "$unit" 2>/dev/null || true)"
    if [[ "$load" == "not-found" || -z "$load" ]]; then
      active="not-found"
      enabled="not-found"
    else
      active="$(systemctl is-active "$unit" 2>/dev/null || true)"
      enabled="$(systemctl is-enabled "$unit" 2>/dev/null || true)"
    fi
    [[ "$first" -eq 1 ]] || printf ','
    first=0
    printf '{"unit":%s,"load":%s,"active":%s,"enabled":%s}' \
      "$(json_escape "$unit")" \
      "$(json_escape "${load:-not-found}")" \
      "$(json_escape "${active:-unknown}")" \
      "$(json_escape "${enabled:-unknown}")"
  done
  printf ']'
}

bucket_json() {
  local first=1 prefix bucket url payload ts count
  printf '['
  for prefix in "${DLP_BUCKET_PREFIXES[@]}"; do
    bucket="${prefix}_${HOSTNAME_FILTER}"
    url="${AW_BASE%/}/api/0/buckets/${bucket}/events?limit=1"
    payload="$(curl -sS --connect-timeout 3 --max-time 8 "$url" 2>/dev/null || true)"
    ts="$(printf '%s' "$payload" | jq -r '.[0].timestamp // ""' 2>/dev/null || true)"
    count="$(printf '%s' "$payload" | jq -r 'if type == "array" then length else 0 end' 2>/dev/null || printf '0')"
    [[ "$first" -eq 1 ]] || printf ','
    first=0
    printf '{"bucket":%s,"sample_count":%s,"latest_timestamp":%s}' \
      "$(json_escape "$bucket")" \
      "${count:-0}" \
      "$(json_escape "$ts")"
  done
  printf ']'
}

unit_exists() {
  local unit="$1"
  systemctl list-unit-files "$unit" --no-legend 2>/dev/null | grep -q . || systemctl status "$unit" >/dev/null 2>&1
}

stop_disable_all_dlp() {
  local unit
  for unit in "${DLP_UNITS[@]}"; do
    if unit_exists "$unit"; then
      systemctl stop "$unit" >/dev/null 2>&1 || true
      systemctl disable "$unit" >/dev/null 2>&1 || true
      systemctl reset-failed "$unit" >/dev/null 2>&1 || true
    fi
  done
}

enable_start_units() {
  local unit
  for unit in "$@"; do
    if unit_exists "$unit"; then
      systemctl enable --now "$unit" >/dev/null 2>&1 || true
    fi
  done
}

capture_rollback_state() {
  local tmp unit load active enabled
  mkdir -p "$STATE_DIR"
  tmp="$(mktemp "${ROLLBACK_FILE}.tmp.XXXXXX")"
  {
    printf '# generated_at_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '# reason=pre_profile_change\n'
    for unit in "${DLP_UNITS[@]}"; do
      load="$(systemctl show -p LoadState --value "$unit" 2>/dev/null || true)"
      if [[ "$load" == "not-found" || -z "$load" ]]; then
        active="not-found"
        enabled="not-found"
      else
        active="$(systemctl is-active "$unit" 2>/dev/null || true)"
        enabled="$(systemctl is-enabled "$unit" 2>/dev/null || true)"
      fi
      printf '%s|%s|%s|%s\n' "$unit" "${load:-not-found}" "$active" "$enabled"
    done
  } >"$tmp"
  mv "$tmp" "$ROLLBACK_FILE"
}

write_stats() {
  local mode="${1:-current}" now stamp tmp history_file
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -p "$STATE_DIR" "$STATE_HISTORY_DIR"
  tmp="$(mktemp "${STATE_FILE}.tmp.XXXXXX")"
  {
    printf '{'
    printf '"generated_at_utc":%s,' "$(json_escape "$now")"
    printf '"mode":%s,' "$(json_escape "$mode")"
    printf '"profile":%s,' "$(json_escape "${AW_DLP_PROFILE:-$PROFILE}")"
    printf '"reason":%s,' "$(json_escape "$REASON")"
    printf '"aw_base":%s,' "$(json_escape "$AW_BASE")"
    printf '"hostname":%s,' "$(json_escape "$HOSTNAME_FILTER")"
    printf '"units":'
    unit_json
    printf ',"buckets":'
    bucket_json
    printf '}\n'
  } >"$tmp"
  mv "$tmp" "$STATE_FILE"
  history_file="${STATE_HISTORY_DIR}/dlp-runtime-${mode}-${stamp}.json"
  cp -a "$STATE_FILE" "$history_file"
  printf 'latest=%s\nhistory=%s\n' "$STATE_FILE" "$history_file"
}

apply_profile() {
  local target_profile="$1"
  capture_rollback_state
  case "$target_profile" in
    core_only|disabled|off)
      PROFILE="core_only"
      stop_disable_all_dlp
      AW_DLP_PROFILE="core_only" write_stats "disabled"
      ;;
    light)
      PROFILE="light"
      stop_disable_all_dlp
      enable_start_units "${DLP_LIGHT_UNITS[@]}"
      AW_DLP_PROFILE="light" write_stats "enabled_light"
      ;;
    on_demand)
      PROFILE="on_demand"
      stop_disable_all_dlp
      enable_start_units "${DLP_ON_DEMAND_UNITS[@]}"
      AW_DLP_PROFILE="on_demand" write_stats "enabled_on_demand"
      ;;
    full|enabled|on)
      PROFILE="full"
      stop_disable_all_dlp
      enable_start_units "${DLP_LIGHT_UNITS[@]}"
      enable_start_units \
        aw-dlp-influx-exporter.timer \
        activitywatch-dlp-aggregator.timer \
        aw-dlp-report-scheduler.timer \
        aw-dlp-syslog-forwarder.timer \
        aw-dlp-webhook-sender.timer \
        aw-dlp-cef-exporter.timer \
        aw-dlp-policy-engine.service \
        aw-dlp-case-management.service \
        detmir-portal-evidence.service
      AW_DLP_PROFILE="full" write_stats "enabled_full"
      ;;
    *)
      printf 'unsupported DLP profile: %s\n' "$target_profile" >&2
      printf 'supported profiles: core_only, light, on_demand, full\n' >&2
      exit 2
      ;;
  esac
}

disable_dlp() {
  apply_profile "core_only"
}

enable_dlp() {
  apply_profile "full"
}

rollback_dlp() {
  local unit load active enabled
  if [[ ! -s "$ROLLBACK_FILE" ]]; then
    printf 'rollback state not found: %s\n' "$ROLLBACK_FILE" >&2
    exit 1
  fi
  stop_disable_all_dlp
  while IFS='|' read -r unit load active enabled; do
    [[ -n "${unit:-}" && "${unit:0:1}" != "#" ]] || continue
    [[ "$load" != "not-found" ]] || continue
    if [[ "$enabled" == "enabled" ]]; then
      systemctl enable "$unit" >/dev/null 2>&1 || true
    fi
    if [[ "$active" == "active" ]]; then
      systemctl start "$unit" >/dev/null 2>&1 || true
    fi
  done <"$ROLLBACK_FILE"
  write_stats "rollback"
}

case "$ACTION" in
  status|stats)
    write_stats "current"
    ;;
  profile)
    printf '%s\n' "${AW_DLP_PROFILE:-$PROFILE}"
    ;;
  set-profile)
    apply_profile "$PROFILE"
    ;;
  disable)
    disable_dlp
    ;;
  enable)
    enable_dlp
    ;;
  rollback)
    rollback_dlp
    ;;
  *)
    printf 'Usage: %s [status|stats|profile|set-profile <core_only|light|on_demand|full>|disable|enable|rollback]\n' "$0" >&2
    exit 2
    ;;
esac
