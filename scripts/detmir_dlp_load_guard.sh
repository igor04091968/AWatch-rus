#!/usr/bin/env bash
set -euo pipefail

ENABLED="${AW_DLP_GUARD_ENABLED:-true}"
PROFILE="${AW_DLP_PROFILE:-light}"
STATE_DIR="${AW_DLP_GUARD_STATE_DIR:-/var/lib/activitywatch/health}"
STATE_FILE="${AW_DLP_GUARD_STATE_FILE:-${STATE_DIR}/dlp-light-guard-state.json}"
STATE_HISTORY_DIR="${AW_DLP_GUARD_HISTORY_DIR:-${STATE_DIR}/dlp-light-guard-history}"
CONTROL_BIN="${AW_DLP_CONTROL_BIN:-/usr/local/bin/detmir-dlp-runtime-control}"
LOAD_RATIO="${AW_DLP_GUARD_LOAD_RATIO:-1.50}"
MEM_AVAILABLE_PCT_MIN="${AW_DLP_GUARD_MEM_AVAILABLE_PCT_MIN:-15}"
IOWAIT_PCT_MAX="${AW_DLP_GUARD_IOWAIT_PCT_MAX:-20}"
STRIKES_REQUIRED="${AW_DLP_GUARD_STRIKES_REQUIRED:-3}"

DLP_GUARDED_UNITS=(
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

json_string() {
  python3 -c 'import json,sys; print(json.dumps(sys.argv[1], ensure_ascii=False))' "$1"
}

number_or_null() {
  local value="${1:-}"
  if [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?$ ]]; then
    printf '%s' "$value"
  else
    printf 'null'
  fi
}

active_dlp_units_json() {
  local first=1 unit
  printf '['
  if command -v systemctl >/dev/null 2>&1; then
    for unit in "${DLP_GUARDED_UNITS[@]}"; do
      if systemctl is-active --quiet "$unit" 2>/dev/null; then
        [[ "$first" -eq 1 ]] || printf ','
        first=0
        json_string "$unit"
      fi
    done
  fi
  printf ']'
}

active_dlp_unit_count() {
  local count=0 unit
  if command -v systemctl >/dev/null 2>&1; then
    for unit in "${DLP_GUARDED_UNITS[@]}"; do
      if systemctl is-active --quiet "$unit" 2>/dev/null; then
        count=$((count + 1))
      fi
    done
  fi
  printf '%s\n' "$count"
}

read_load1() {
  awk '{print $1}' /proc/loadavg 2>/dev/null || printf '0'
}

read_cpu_count() {
  local cores
  cores="$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '1')"
  if [[ ! "$cores" =~ ^[0-9]+$ || "$cores" -lt 1 ]]; then
    cores=1
  fi
  printf '%s\n' "$cores"
}

read_mem_available_pct() {
  awk '
    /^MemTotal:/ { total=$2 }
    /^MemAvailable:/ { available=$2 }
    END {
      if (total > 0) {
        printf "%.2f", (available * 100.0 / total)
      } else {
        printf "0"
      }
    }
  ' /proc/meminfo 2>/dev/null || printf '0'
}

read_cpu_sample() {
  awk '/^cpu / {
    idle=$5
    iowait=$6
    total=0
    for (i=2; i<=NF; i++) total += $i
    printf "%s %s\n", total, iowait
    exit
  }' /proc/stat 2>/dev/null || printf '0 0'
}

read_iowait_pct() {
  local total1 wait1 total2 wait2 dtotal dwait
  read -r total1 wait1 < <(read_cpu_sample)
  sleep 1
  read -r total2 wait2 < <(read_cpu_sample)
  dtotal=$((total2 - total1))
  dwait=$((wait2 - wait1))
  if [[ "$dtotal" -le 0 || "$dwait" -lt 0 ]]; then
    printf '0'
    return
  fi
  awk -v wait="$dwait" -v total="$dtotal" 'BEGIN { printf "%.2f", wait * 100.0 / total }'
}

is_over_threshold() {
  local value="$1"
  local threshold="$2"
  awk -v value="$value" -v threshold="$threshold" 'BEGIN { exit !(value > threshold) }'
}

is_under_threshold() {
  local value="$1"
  local threshold="$2"
  awk -v value="$value" -v threshold="$threshold" 'BEGIN { exit !(value < threshold) }'
}

write_state() {
  local action="$1"
  local reason="$2"
  local load1="$3"
  local cores="$4"
  local load_threshold="$5"
  local mem_pct="$6"
  local iowait_pct="$7"
  local active_count="$8"
  local active_units_json="$9"
  local control_exit="${10}"
  local strikes="${11:-0}"
  local now stamp tmp history

  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -p "$STATE_DIR" "$STATE_HISTORY_DIR"
  tmp="$(mktemp "${STATE_FILE}.tmp.XXXXXX")"
  {
    printf '{'
    printf '"generated_at_utc":%s,' "$(json_string "$now")"
    printf '"profile":%s,' "$(json_string "$PROFILE")"
    printf '"guard_enabled":%s,' "$(json_string "$ENABLED")"
    printf '"action":%s,' "$(json_string "$action")"
    printf '"reason":%s,' "$(json_string "$reason")"
    printf '"consecutive_overload_count":%s,' "$(number_or_null "$strikes")"
    printf '"consecutive_overload_required":%s,' "$(number_or_null "$STRIKES_REQUIRED")"
    printf '"control_bin":%s,' "$(json_string "$CONTROL_BIN")"
    printf '"control_exit":%s,' "$(number_or_null "$control_exit")"
    printf '"metrics":{'
    printf '"load1":%s,' "$(number_or_null "$load1")"
    printf '"cpu_count":%s,' "$(number_or_null "$cores")"
    printf '"load_threshold":%s,' "$(number_or_null "$load_threshold")"
    printf '"mem_available_pct":%s,' "$(number_or_null "$mem_pct")"
    printf '"mem_available_pct_min":%s,' "$(number_or_null "$MEM_AVAILABLE_PCT_MIN")"
    printf '"iowait_pct":%s,' "$(number_or_null "$iowait_pct")"
    printf '"iowait_pct_max":%s' "$(number_or_null "$IOWAIT_PCT_MAX")"
    printf '},'
    printf '"active_dlp_unit_count":%s,' "$(number_or_null "$active_count")"
    printf '"active_dlp_units":%s' "$active_units_json"
    printf '}\n'
  } >"$tmp"
  mv "$tmp" "$STATE_FILE"
  history="${STATE_HISTORY_DIR}/dlp-light-guard-${stamp}.json"
  cp -a "$STATE_FILE" "$history"
  printf 'dlp guard action=%s reason=%s state=%s history=%s\n' "$action" "$reason" "$STATE_FILE" "$history"
}

main() {
  local load1 cores load_threshold mem_pct iowait_pct active_count active_units_json overloaded reason control_exit strikes prev_strikes

  load1="$(read_load1)"
  cores="$(read_cpu_count)"
  load_threshold="$(awk -v cores="$cores" -v ratio="$LOAD_RATIO" 'BEGIN { printf "%.2f", cores * ratio }')"
  mem_pct="$(read_mem_available_pct)"
  iowait_pct="$(read_iowait_pct)"
  active_units_json="$(active_dlp_units_json)"
  active_count="$(active_dlp_unit_count)"
  overloaded=0
  reason="within_thresholds"
  prev_strikes="$(
    python3 - "$STATE_FILE" <<'PY' 2>/dev/null || true
import json, sys
try:
    print(int(json.load(open(sys.argv[1])).get("consecutive_overload_count", 0)))
except Exception:
    print(0)
PY
  )"
  [[ "$prev_strikes" =~ ^[0-9]+$ ]] || prev_strikes=0
  strikes=0

  if is_over_threshold "$load1" "$load_threshold"; then
    overloaded=1
    reason="load1_above_threshold"
  elif is_under_threshold "$mem_pct" "$MEM_AVAILABLE_PCT_MIN"; then
    overloaded=1
    reason="mem_available_below_threshold"
  elif is_over_threshold "$iowait_pct" "$IOWAIT_PCT_MAX"; then
    overloaded=1
    reason="iowait_above_threshold"
  fi

  if [[ "$ENABLED" != "true" && "$ENABLED" != "1" && "$ENABLED" != "yes" ]]; then
    write_state "skipped" "guard_disabled" "$load1" "$cores" "$load_threshold" "$mem_pct" "$iowait_pct" "$active_count" "$active_units_json" "0" "0"
    return 0
  fi

  if [[ "$overloaded" -eq 0 ]]; then
    write_state "none" "$reason" "$load1" "$cores" "$load_threshold" "$mem_pct" "$iowait_pct" "$active_count" "$active_units_json" "0" "0"
    return 0
  fi

  strikes=$((prev_strikes + 1))

  if [[ "$strikes" -lt "$STRIKES_REQUIRED" ]]; then
    write_state "observe_overload" "$reason" "$load1" "$cores" "$load_threshold" "$mem_pct" "$iowait_pct" "$active_count" "$active_units_json" "0" "$strikes"
    return 0
  fi

  if [[ "$active_count" -eq 0 ]]; then
    write_state "none" "${reason}_but_no_active_dlp_units" "$load1" "$cores" "$load_threshold" "$mem_pct" "$iowait_pct" "$active_count" "$active_units_json" "0" "$strikes"
    return 0
  fi

  if [[ ! -x "$CONTROL_BIN" ]]; then
    write_state "failed" "${reason}_control_bin_missing" "$load1" "$cores" "$load_threshold" "$mem_pct" "$iowait_pct" "$active_count" "$active_units_json" "127" "$strikes"
    printf 'DLP guard cannot disable overloaded DLP: executable not found: %s\n' "$CONTROL_BIN" >&2
    return 127
  fi

  control_exit=0
  AW_DLP_DISABLED_REASON="auto_disabled_by_dlp_load_guard:${reason}" "$CONTROL_BIN" set-profile core_only || control_exit=$?
  if [[ "$control_exit" -eq 0 ]]; then
    write_state "auto_disabled" "$reason" "$load1" "$cores" "$load_threshold" "$mem_pct" "$iowait_pct" "$active_count" "$active_units_json" "$control_exit" "$strikes"
  else
    write_state "failed" "${reason}_control_exit_${control_exit}" "$load1" "$cores" "$load_threshold" "$mem_pct" "$iowait_pct" "$active_count" "$active_units_json" "$control_exit" "$strikes"
  fi
  return "$control_exit"
}

main "$@"
