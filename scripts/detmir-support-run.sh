#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

LOCAL_ENV_FILE="$SCRIPT_DIR/detmir-support.env"
USER_ENV_FILE="${XDG_CONFIG_HOME:-$HOME/.config}/awatch-rus/detmir-support.env"
if [[ -n "${DETMIR_SUPPORT_ENV_FILE:-}" ]]; then
  ENV_FILE="$DETMIR_SUPPORT_ENV_FILE"
elif [[ -f "$LOCAL_ENV_FILE" ]]; then
  ENV_FILE="$LOCAL_ENV_FILE"
else
  ENV_FILE="$USER_ENV_FILE"
fi

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

usage() {
  cat <<'USAGE'
Usage:
  scripts/detmir-support-run.sh --scope daily|weekly|monthly [--output-dir PATH]
  scripts/detmir-support-daily.sh
  scripts/detmir-support-weekly.sh
  scripts/detmir-support-monthly.sh

Arguments:
  --scope          daily|weekly|monthly (default: daily)
  --output-dir     directory for logs/reports (default: output/detmir-support)
  --help           show help
USAGE
}

SCOPE="daily"
OUTPUT_DIR="${DETMIR_SUPPORT_OUTPUT_DIR:-$REPO_ROOT/output/detmir-support}"

while (( $# > 0 )); do
  case "$1" in
    --scope)
      if (( $# < 2 )); then
        usage
        echo "error: --scope requires daily|weekly|monthly" >&2
        exit 2
      fi
      SCOPE="$2"
      shift 2
      ;;
    --output-dir)
      if (( $# < 2 )); then
        usage
        echo "error: --output-dir requires directory path" >&2
        exit 2
      fi
      OUTPUT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      echo "error: unknown argument '$1'" >&2
      exit 2
      ;;
  esac
done

if [[ "$SCOPE" != "daily" && "$SCOPE" != "weekly" && "$SCOPE" != "monthly" ]]; then
  echo "error: invalid scope '$SCOPE', expected daily|weekly|monthly" >&2
  exit 2
fi

DETMIR_SUPPORT_PVE_HOST="${DETMIR_SUPPORT_PVE_HOST:-10.10.10.2}"
DETMIR_SUPPORT_AW_HOST="${DETMIR_SUPPORT_AW_HOST:-10.10.10.13}"
DETMIR_SUPPORT_WEB_HOST="${DETMIR_SUPPORT_WEB_HOST:-$DETMIR_SUPPORT_PVE_HOST}"
DETMIR_SUPPORT_WINDOWS_HOST="${DETMIR_SUPPORT_WINDOWS_HOST:-192.168.100.19}"
DETMIR_SUPPORT_PFSENSE_HOST="${DETMIR_SUPPORT_PFSENSE_HOST:-}"
DETMIR_SUPPORT_PFSENSE_URL="${DETMIR_SUPPORT_PFSENSE_URL:-}"
DETMIR_SUPPORT_OPENVPN_HOST="${DETMIR_SUPPORT_OPENVPN_HOST:-}"
DETMIR_SUPPORT_OPENVPN_WEB_URL="${DETMIR_SUPPORT_OPENVPN_WEB_URL:-}"
DETMIR_SUPPORT_SURICATA_HOST="${DETMIR_SUPPORT_SURICATA_HOST:-$DETMIR_SUPPORT_PVE_HOST}"
DETMIR_SUPPORT_WEB_TLS_HOST="${DETMIR_SUPPORT_WEB_TLS_HOST:-$DETMIR_SUPPORT_WEB_HOST}"

DETMIR_SUPPORT_SSH_USER="${DETMIR_SUPPORT_SSH_USER:-igor}"
DETMIR_SUPPORT_PVE_SSH_USER="${DETMIR_SUPPORT_PVE_SSH_USER:-$DETMIR_SUPPORT_SSH_USER}"
DETMIR_SUPPORT_AW_SSH_USER="${DETMIR_SUPPORT_AW_SSH_USER:-$DETMIR_SUPPORT_SSH_USER}"
DETMIR_SUPPORT_WEB_SSH_USER="${DETMIR_SUPPORT_WEB_SSH_USER:-$DETMIR_SUPPORT_SSH_USER}"
DETMIR_SUPPORT_SURICATA_SSH_USER="${DETMIR_SUPPORT_SURICATA_SSH_USER:-$DETMIR_SUPPORT_PVE_SSH_USER}"
DETMIR_SUPPORT_SSH_PASSWORD="${DETMIR_SUPPORT_SSH_PASSWORD:-}"
DETMIR_SUPPORT_PVE_SSH_PASSWORD="${DETMIR_SUPPORT_PVE_SSH_PASSWORD:-$DETMIR_SUPPORT_SSH_PASSWORD}"
DETMIR_SUPPORT_AW_SSH_PASSWORD="${DETMIR_SUPPORT_AW_SSH_PASSWORD:-$DETMIR_SUPPORT_SSH_PASSWORD}"
DETMIR_SUPPORT_WEB_SSH_PASSWORD="${DETMIR_SUPPORT_WEB_SSH_PASSWORD:-$DETMIR_SUPPORT_SSH_PASSWORD}"
DETMIR_SUPPORT_SURICATA_SSH_PASSWORD="${DETMIR_SUPPORT_SURICATA_SSH_PASSWORD:-$DETMIR_SUPPORT_PVE_SSH_PASSWORD}"
DETMIR_SUPPORT_SSH_IDENTITY="${DETMIR_SUPPORT_SSH_IDENTITY:-}"
DETMIR_SUPPORT_PVE_SSH_IDENTITY="${DETMIR_SUPPORT_PVE_SSH_IDENTITY:-$DETMIR_SUPPORT_SSH_IDENTITY}"
DETMIR_SUPPORT_AW_SSH_IDENTITY="${DETMIR_SUPPORT_AW_SSH_IDENTITY:-$DETMIR_SUPPORT_SSH_IDENTITY}"
DETMIR_SUPPORT_WEB_SSH_IDENTITY="${DETMIR_SUPPORT_WEB_SSH_IDENTITY:-$DETMIR_SUPPORT_SSH_IDENTITY}"
DETMIR_SUPPORT_SURICATA_SSH_IDENTITY="${DETMIR_SUPPORT_SURICATA_SSH_IDENTITY:-$DETMIR_SUPPORT_PVE_SSH_IDENTITY}"
DETMIR_SUPPORT_SKIP_REMOTE="${DETMIR_SUPPORT_SKIP_REMOTE:-0}"
DETMIR_SUPPORT_CONNECT_TIMEOUT="${DETMIR_SUPPORT_CONNECT_TIMEOUT:-8}"

DETMIR_SUPPORT_PVE_SERVICES="${DETMIR_SUPPORT_PVE_SERVICES:-pve-cluster pvedaemon pvestatd pveproxy pvedaemon.service pvestatd.service}"
DETMIR_SUPPORT_AW_SERVICES="${DETMIR_SUPPORT_AW_SERVICES:-activitywatch-server aw-server-rust}"
DETMIR_SUPPORT_WEB_SERVICES="${DETMIR_SUPPORT_WEB_SERVICES:-nginx}"
DETMIR_SUPPORT_SURICATA_SERVICES="${DETMIR_SUPPORT_SURICATA_SERVICES:-suricata}"
DETMIR_SUPPORT_PVE_VM_IDS="${DETMIR_SUPPORT_PVE_VM_IDS:-}"
DETMIR_SUPPORT_PVE_BACKUP_DIRS="${DETMIR_SUPPORT_PVE_BACKUP_DIRS:-/var/lib/pve/local-btrfs/dump}"
DETMIR_SUPPORT_BACKUP_MAX_AGE_DAYS="${DETMIR_SUPPORT_BACKUP_MAX_AGE_DAYS:-14}"

DETMIR_SUPPORT_DISK_WARN_PERCENT="${DETMIR_SUPPORT_DISK_WARN_PERCENT:-85}"
DETMIR_SUPPORT_DISK_CRIT_PERCENT="${DETMIR_SUPPORT_DISK_CRIT_PERCENT:-93}"
DETMIR_SUPPORT_LOG_WARNING_LIMIT="${DETMIR_SUPPORT_LOG_WARNING_LIMIT:-5}"
DETMIR_SUPPORT_LOG_CRIT_LIMIT="${DETMIR_SUPPORT_LOG_CRIT_LIMIT:-20}"
DETMIR_SUPPORT_TLS_WARN_DAYS="${DETMIR_SUPPORT_TLS_WARN_DAYS:-30}"
DETMIR_SUPPORT_TLS_CRIT_DAYS="${DETMIR_SUPPORT_TLS_CRIT_DAYS:-14}"
DETMIR_SUPPORT_HTTP_WARNING_LATENCY_MS="${DETMIR_SUPPORT_HTTP_WARNING_LATENCY_MS:-2000}"

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="$OUTPUT_DIR/$(date -u +%F)"
mkdir -p "$RUN_DIR"

LOG_FILE="$RUN_DIR/support-$SCOPE-$RUN_ID.log"
CSV_FILE="$RUN_DIR/support-$SCOPE-$RUN_ID.csv"
MD_FILE="$RUN_DIR/support-$SCOPE-$RUN_ID.md"

TOTAL=0
OK_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
REMOTE_OUTPUT=""
REMOTE_ERROR=""

printf '%s\n' "timestamp;scope;section;status;check;result;action" > "$CSV_FILE"

log() {
  printf '%s %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" | tee -a "$LOG_FILE"
}

sanitize() {
  local value="$1"
  value="${value//$'\n'/ }"
  value="${value//$'\r'/ }"
  value="${value//;/ }"
  printf '%s' "$value"
}

record() {
  local section="$1"
  local status="$2"
  local check_name="$3"
  local result="$4"
  local action="${5:-}"

  section="$(sanitize "$section")"
  check_name="$(sanitize "$check_name")"
  status="$(sanitize "$status")"
  result="$(sanitize "$result")"
  action="$(sanitize "$action")"

  TOTAL=$((TOTAL + 1))
  case "$status" in
    OK) OK_COUNT=$((OK_COUNT + 1)) ;;
    WARN) WARN_COUNT=$((WARN_COUNT + 1)) ;;
    FAIL) FAIL_COUNT=$((FAIL_COUNT + 1)) ;;
    SKIP) SKIP_COUNT=$((SKIP_COUNT + 1)) ;;
  esac

  printf '%s;%s;%s;%s;%s;%s;%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$SCOPE" "$section" "$status" "$check_name" "$result" "$action" >> "$CSV_FILE"
  case "$status" in
    OK) log "[OK]   ${section} :: ${check_name} -- ${result}" ;;
    WARN) log "[WARN] ${section} :: ${check_name} -- ${result}" ;;
    FAIL) log "[FAIL] ${section} :: ${check_name} -- ${result}" ;;
    SKIP) log "[SKIP] ${section} :: ${check_name} -- ${result}" ;;
  esac
  if [[ -n "$action" ]]; then
    log "       action: $action"
  fi
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    record "Подготовка" FAIL "$1" "not found" "Install command and re-run"
    return 1
  fi
}

remote_failure_summary() {
  local fallback="${1:-Remote command failed}"
  local detail=""
  if [[ -n "$REMOTE_ERROR" ]]; then
    detail="$REMOTE_ERROR"
  elif [[ -n "$REMOTE_OUTPUT" ]]; then
    detail="$REMOTE_OUTPUT"
  else
    detail="$fallback"
  fi
  detail="$(printf '%s\n' "$detail" | grep -Ev '^(Warning: Permanently added|$)' | head -n 1 || true)"
  if [[ -z "$detail" ]]; then
    detail="$fallback"
  fi
  sanitize "$detail"
}

remote_user_for_host() {
  local host="$1"
  if [[ "$host" == "$DETMIR_SUPPORT_PVE_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_PVE_SSH_USER"
  elif [[ "$host" == "$DETMIR_SUPPORT_AW_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_AW_SSH_USER"
  elif [[ "$host" == "$DETMIR_SUPPORT_WEB_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_WEB_SSH_USER"
  elif [[ "$host" == "$DETMIR_SUPPORT_SURICATA_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_SURICATA_SSH_USER"
  else
    printf '%s' "$DETMIR_SUPPORT_SSH_USER"
  fi
}

remote_password_for_host() {
  local host="$1"
  if [[ "$host" == "$DETMIR_SUPPORT_PVE_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_PVE_SSH_PASSWORD"
  elif [[ "$host" == "$DETMIR_SUPPORT_AW_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_AW_SSH_PASSWORD"
  elif [[ "$host" == "$DETMIR_SUPPORT_WEB_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_WEB_SSH_PASSWORD"
  elif [[ "$host" == "$DETMIR_SUPPORT_SURICATA_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_SURICATA_SSH_PASSWORD"
  else
    printf '%s' "$DETMIR_SUPPORT_SSH_PASSWORD"
  fi
}

remote_identity_for_host() {
  local host="$1"
  if [[ "$host" == "$DETMIR_SUPPORT_PVE_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_PVE_SSH_IDENTITY"
  elif [[ "$host" == "$DETMIR_SUPPORT_AW_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_AW_SSH_IDENTITY"
  elif [[ "$host" == "$DETMIR_SUPPORT_WEB_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_WEB_SSH_IDENTITY"
  elif [[ "$host" == "$DETMIR_SUPPORT_SURICATA_HOST" ]]; then
    printf '%s' "$DETMIR_SUPPORT_SURICATA_SSH_IDENTITY"
  else
    printf '%s' "$DETMIR_SUPPORT_SSH_IDENTITY"
  fi
}

run_remote() {
  local host="$1"
  shift
  local remote_cmd="$*"

  if [[ "$DETMIR_SUPPORT_SKIP_REMOTE" == "1" ]]; then
    return 125
  fi

  local ssh_user ssh_password ssh_identity
  ssh_user="$(remote_user_for_host "$host")"
  ssh_password="$(remote_password_for_host "$host")"
  ssh_identity="$(remote_identity_for_host "$host")"

  local -a ssh_opts=(
    -o ConnectTimeout="$DETMIR_SUPPORT_CONNECT_TIMEOUT"
    -o StrictHostKeyChecking=no
    -o UserKnownHostsFile=/dev/null
  )
  if [[ -n "$ssh_password" ]]; then
    ssh_opts=(-o BatchMode=no "${ssh_opts[@]}")
  else
    ssh_opts=(-o BatchMode=yes "${ssh_opts[@]}")
  fi
  if [[ -n "$ssh_identity" ]]; then
    ssh_opts=("-i" "$ssh_identity" "${ssh_opts[@]}")
  fi

  local stderr_file rc
  stderr_file="$(mktemp)"
  REMOTE_OUTPUT=""
  REMOTE_ERROR=""
  local -a ssh_cmd=(ssh "${ssh_opts[@]}" "${ssh_user}@${host}" bash -s)
  if [[ -n "$ssh_password" ]]; then
    if ! command -v sshpass >/dev/null 2>&1; then
      REMOTE_ERROR="sshpass is required for password authentication"
      rm -f "$stderr_file"
      return 127
    fi
    ssh_cmd=(sshpass -e "${ssh_cmd[@]}")
  fi

  if REMOTE_OUTPUT="$(printf '%s\n' "$remote_cmd" | SSHPASS="$ssh_password" "${ssh_cmd[@]}" 2>"$stderr_file")"; then
    REMOTE_ERROR="$(<"$stderr_file")"
    rm -f "$stderr_file"
    if echo "$REMOTE_ERROR" | grep -Eiq "Permission denied|Connection refused|No route to host|Connection timed out|Could not resolve hostname"; then
      return 255
    fi
    return 0
  fi
  rc=$?
  REMOTE_ERROR="$(<"$stderr_file")"
  rm -f "$stderr_file"
  return "$rc"
}

check_tcp() {
  local section="$1" host="$2" port="$3" name="$4"
  if timeout 8 bash -c ": >/dev/tcp/$host/$port" >/dev/null 2>&1; then
    record "$section" OK "$name" "${host}:${port} reachable"
  else
    record "$section" FAIL "$name" "${host}:${port} unreachable"
  fi
}

check_http() {
  local section="$1" url="$2" expected_code="$3" name="$4"
  local tmp_file code latency_ms
  tmp_file="$(mktemp)"
  code="$(curl -ksS --connect-timeout 5 --max-time 20 -o "$tmp_file" -w '%{http_code};%{time_total}' "$url" 2>/dev/null || true)"
  latency_ms="${code#*;}"
  code="${code%;*}"
  latency_ms="$(awk "BEGIN { printf \"%.0f\", ${latency_ms:-0} * 1000 }")"
  if [[ -z "$code" ]]; then
    record "$section" WARN "$name" "No HTTP response" "Check service, DNS and auth"
    rm -f "$tmp_file"
    return
  fi

  if echo "$code" | grep -Eq "$expected_code"; then
    if (( latency_ms > DETMIR_SUPPORT_HTTP_WARNING_LATENCY_MS )); then
      record "$section" WARN "$name" "$url -> HTTP ${code}, latency ${latency_ms}ms" "Check service latency/perf"
    else
      record "$section" OK "$name" "$url -> HTTP ${code}, latency ${latency_ms}ms"
    fi
  else
    record "$section" WARN "$name" "$url -> HTTP ${code}" "Verify endpoint auth/cert/rewrite"
  fi
  rm -f "$tmp_file"
}

check_tls() {
  local section="$1" host="$2" port="$3" name="$4"
  if ! command -v openssl >/dev/null 2>&1; then
    record "$section" SKIP "$name" "openssl unavailable" "Install openssl to validate cert"
    return
  fi

  local end_line now_ts end_ts days_left
  end_line="$(timeout 8 sh -c "openssl s_client -connect '$host:$port' -servername '$host' </dev/null 2>/dev/null | openssl x509 -noout -enddate 2>/dev/null | sed -n 's/^notAfter=//p' || true")"
  if [[ -z "$end_line" ]]; then
    record "$section" WARN "$name" "Unable to read certificate" "Check TLS termination"
    return
  fi

  now_ts="$(date -u +%s)"
  end_ts="$(date -d "$end_line" +%s 2>/dev/null || true)"
  if [[ -z "$end_ts" ]]; then
    record "$section" WARN "$name" "Certificate expiry parse failed" "Update openssl/timezone settings"
    return
  fi

  days_left=$(( (end_ts - now_ts) / 86400 ))
  if (( days_left < 0 )); then
    record "$section" FAIL "$name" "Certificate already expired (${days_left#-} days ago)" "Renew certificate immediately"
  elif (( days_left <= DETMIR_SUPPORT_TLS_CRIT_DAYS )); then
    record "$section" FAIL "$name" "Expires in ${days_left} days" "Renew certificate before expiry"
  elif (( days_left <= DETMIR_SUPPORT_TLS_WARN_DAYS )); then
    record "$section" WARN "$name" "Expires in ${days_left} days" "Plan renewal window"
  else
    record "$section" OK "$name" "Expires in ${days_left} days" ""
  fi
}

check_remote_service_status() {
  local section="$1" host="$2" services="$3"
  if [[ -z "$services" ]]; then
    return
  fi

  local service
  for service in $services; do
    if run_remote "$host" "systemctl is-active \"$service\""; then
      local state
      state="$(echo "$REMOTE_OUTPUT" | tr -d '\r' | awk 'NF {print $1; exit}')"
      if [[ "$state" == "active" ]]; then
        record "$section" OK "$service" "$host active" ""
      elif [[ -n "$state" ]]; then
        record "$section" WARN "$service" "$host state=$state" "Check unit status"
      else
        local detail
        detail="$(remote_failure_summary "Empty systemctl output")"
        record "$section" SKIP "$service" "$host empty systemctl output; ${detail}" "Check remote SSH/auth and unit name"
      fi
    else
      local rc=$?
      if (( rc == 125 )); then
        record "$section" SKIP "$service" "SSH skipped" "Provide SSH access or run locally"
      else
        local detail
        detail="$(remote_failure_summary "systemctl failed")"
        record "$section" WARN "$service" "$host systemctl check failed; ${detail}" "Check remote SSH/auth and unit status"
      fi
    fi
  done
}

check_node_load() {
  local section="$1" host="$2"
  if run_remote "$host" "awk '{print \$1 \" \" \$2 \" \" \$3}' /proc/loadavg"; then
    local load_line="$REMOTE_OUTPUT"
    if [[ -z "$load_line" ]]; then
      record "$section" SKIP "Node load" "Empty loadavg output" "Check /proc on remote"
    elif ! [[ "$load_line" =~ ^[0-9]+([.][0-9]+)?[[:space:]][0-9]+([.][0-9]+)?[[:space:]][0-9]+([.][0-9]+)?$ ]]; then
      record "$section" SKIP "Node load" "Unable to parse loadavg output ($(sanitize "$load_line"))" "Check remote shell output"
    else
      local avg1 avg5 avg15
      avg1="${load_line%% *}"
      avg5="$(echo "$load_line" | awk '{print $2}')"
      avg15="$(echo "$load_line" | awk '{print $3}')"
      record "$section" OK "Node load" "1m=$avg1 5m=$avg5 15m=$avg15" ""
    fi
  else
    local rc=$?
    if (( rc == 125 )); then
      record "$section" SKIP "Node load" "SSH skipped" "Provide SSH access or run locally"
    else
      local detail
      detail="$(remote_failure_summary "Cannot read /proc/loadavg")"
      record "$section" WARN "Node load" "$host load check failed; ${detail}" "Check remote SSH/auth and /proc access"
    fi
  fi
}

check_disk() {
  local section="$1" host="$2" mount="$3"
  if run_remote "$host" "df -P '$mount' | awk 'NR==2 {print \$5}'"; then
    local used=0
    local raw_total
    raw_total="$(echo "$REMOTE_OUTPUT" | tr '\\t' ' ')"
    used="${raw_total%%\%*}"
    if ! [[ "$used" =~ ^[0-9]+$ ]]; then
      local detail
      detail="$(remote_failure_summary "Empty or invalid df output")"
      record "$section" SKIP "disk $mount" "Unable to parse df output on ${host}:${mount}; ${detail}" "Check remote SSH/auth, mount and fs output"
      return
    fi
    if (( used >= DETMIR_SUPPORT_DISK_CRIT_PERCENT )); then
      record "$section" FAIL "disk $mount" "Used ${used}% on ${host}:${mount}" "Free space now"
    elif (( used >= DETMIR_SUPPORT_DISK_WARN_PERCENT )); then
      record "$section" WARN "disk $mount" "Used ${used}% on ${host}:${mount}" "Plan cleanup/rebalance"
    else
      record "$section" OK "disk $mount" "Used ${used}% on ${host}:${mount}" ""
    fi
  else
    local rc=$?
    if (( rc == 125 )); then
      record "$section" SKIP "disk $mount" "SSH skipped" "Provide SSH access"
    else
      local detail
      detail="$(remote_failure_summary "df failed")"
      record "$section" FAIL "disk $mount" "$host df failed; ${detail}" "Check remote SSH/auth, permissions and mount"
    fi
  fi
}

check_backups() {
  local section="$1" host="$2" path="$3"
  local remote_cmd
  # shellcheck disable=SC2016
  remote_cmd=$(cat <<EOF
if [ ! -d '$path' ]; then
  echo NO_PATH
  exit 0
fi
find '$path' -type f \( -name 'vzdump-*' -o -name '*.vma*' -o -name '*.tar*' -o -name '*.zip' -o -name '*.bak' \) -printf '%T@ %p\n' 2>/dev/null | sort -nr | head -1 | awk '{print \$1}'
EOF
)
  if run_remote "$host" "$remote_cmd"; then
    if [[ "$REMOTE_OUTPUT" == "NO_PATH" ]]; then
      record "$section" SKIP "backup" "Path not found: ${path}" "Fix backup path in env"
      return
    fi
    if [[ -z "$REMOTE_OUTPUT" ]]; then
      record "$section" WARN "backup" "No backup files found in ${path}" "Check backup schedule"
      return
    fi
    if ! [[ "$REMOTE_OUTPUT" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
      record "$section" SKIP "backup" "Unable to parse backup timestamp ($(sanitize "$REMOTE_OUTPUT"))" "Check remote SSH output and backup path"
      return
    fi
    local epoch_now age_days
    epoch_now="$(date -u +%s)"
    age_days=$(( (epoch_now - ${REMOTE_OUTPUT%.*}) / 86400 ))
    if (( age_days > DETMIR_SUPPORT_BACKUP_MAX_AGE_DAYS )); then
      record "$section" WARN "backup" "Last backup ${age_days} day(s) ago" "Check scheduler and retention"
    else
      record "$section" OK "backup" "Last backup ${age_days} day(s) ago" ""
    fi
  else
    local rc=$?
    if (( rc == 125 )); then
      record "$section" SKIP "backup" "SSH skipped" "Provide SSH access"
    else
      local detail
      detail="$(remote_failure_summary "Backup check failed")"
      record "$section" FAIL "backup" "$host backup check failed; ${detail}" "Inspect remote SSH/auth, permissions and path"
    fi
  fi
}

check_vms() {
  local section="$1" host="$2" vm_ids="$3"
  if [[ -z "$vm_ids" ]]; then
    return
  fi

  if run_remote "$host" "if command -v qm >/dev/null 2>&1; then qm list; else echo NO_QM; fi"; then
    if [[ "$REMOTE_OUTPUT" == "NO_QM" ]]; then
      record "$section" SKIP "VM/CT state" "qm command not found" "Run checks only on Proxmox host"
      return
    fi
    local vm_id state
    for vm_id in $vm_ids; do
      state="$(echo "$REMOTE_OUTPUT" | awk -v id="$vm_id" '$1==id {print $3}')"
      if [[ -z "$state" ]]; then
        record "$section" WARN "VM/CT $vm_id" "Not found in qm list" "Verify ID list"
      elif [[ "$state" == "running" ]]; then
        record "$section" OK "VM/CT $vm_id" "running" ""
      else
        record "$section" WARN "VM/CT $vm_id" "state=$state" "Inspect console before maintenance"
      fi
    done
  else
    local rc=$?
    if (( rc == 125 )); then
      record "$section" SKIP "VM/CT state" "SSH skipped" "Provide SSH access"
    else
      record "$section" FAIL "VM/CT state" "Cannot run qm list" "Check Proxmox shell"
    fi
  fi
}

check_task_log() {
  local section="$1" host="$2"
  if run_remote "$host" "journalctl -p 3 --no-pager -n 50 2>/dev/null | grep -Eci '(error|fail|critical|timeout)' || true"; then
    local errors="${REMOTE_OUTPUT//[$'\n']/ }"
    if ! [[ "$errors" =~ ^[0-9]+$ ]]; then
      record "$section" SKIP "task log" "Cannot parse journal count" "Check journald"
      return
    fi
    if (( errors > 10 )); then
      record "$section" WARN "task log" "Found ${errors} critical/error lines" "Review pve/task.log scope"
    else
      record "$section" OK "task log" "Critical/error lines: ${errors}" ""
    fi
  else
    local rc=$?
    if (( rc == 125 )); then
      record "$section" SKIP "task log" "SSH skipped" "Provide SSH access"
    else
      record "$section" WARN "task log" "Cannot read task log" "Check journald config"
    fi
  fi
}

check_log_growth() {
  local section="$1" host="$2"
  local dir total dir_count
  total=0

  for dir in /var/log /var/log/pve /var/log/nginx; do
    if run_remote "$host" "[ -d '$dir' ] && find '$dir' -type f -size +100M 2>/dev/null | wc -l"; then
      dir_count="${REMOTE_OUTPUT//[$'\n']/ }"
      dir_count="${dir_count// /}"
      if ! [[ "$dir_count" =~ ^[0-9]+$ ]]; then
        dir_count=0
      fi
      total=$((total + dir_count))
    else
      local rc=$?
      if (( rc == 125 )); then
        record "$section" SKIP "log growth" "SSH skipped" "Provide SSH access"
      else
        record "$section" FAIL "log growth" "scan failed" "Check permissions"
      fi
      return
    fi
  done

  local large_files="${total}"
  if (( large_files >= DETMIR_SUPPORT_LOG_CRIT_LIMIT )); then
    record "$section" WARN "log growth" "${large_files} files >100M" "Run cleanup/logrotate"
  elif (( large_files >= DETMIR_SUPPORT_LOG_WARNING_LIMIT )); then
    record "$section" WARN "log growth" "${large_files} files >100M" "Monitor growth"
  else
    record "$section" OK "log growth" "${large_files} files >100M" ""
  fi
}

check_pfsense() {
  local section="$1"
  if [[ -n "$DETMIR_SUPPORT_PFSENSE_URL" ]]; then
    check_http "$section" "$DETMIR_SUPPORT_PFSENSE_URL" '^2[0-9][0-9]$' "pfSense Web"
  fi
  if [[ -n "$DETMIR_SUPPORT_PFSENSE_HOST" ]]; then
    check_tcp "$section" "$DETMIR_SUPPORT_PFSENSE_HOST" 443 "pfSense HTTPS"
  fi
}

check_openvpn() {
  local section="$1"
  if [[ -n "$DETMIR_SUPPORT_OPENVPN_WEB_URL" ]]; then
    check_http "$section" "$DETMIR_SUPPORT_OPENVPN_WEB_URL" '^2[0-9][0-9]$' "OpenVPN Web"
  fi
  if [[ -n "$DETMIR_SUPPORT_OPENVPN_HOST" ]]; then
    check_tcp "$section" "$DETMIR_SUPPORT_OPENVPN_HOST" 1194 "OpenVPN UDP/TCP"
  fi
}

check_suricata_service() {
  local section="$1" host="$2"
  check_remote_service_status "$section" "$host" "$DETMIR_SUPPORT_SURICATA_SERVICES"

  if run_remote "$host" "systemctl is-active suricata 2>/dev/null || true"; then
    local service_state
    service_state="$(echo "$REMOTE_OUTPUT" | tr -d '\r' | awk 'NF {print $1; exit}')"
    if [[ -n "$service_state" && "$service_state" != "active" ]]; then
      record "$section" SKIP "Suricata process" "Skipped because suricata.service state=${service_state}" "Start Suricata only if IDS/IPS is required on this host"
      return
    fi
  fi

  if run_remote "$host" "command -v suricata >/dev/null 2>&1 && pgrep -af suricata >/dev/null && echo running"; then
    if [[ "$(echo "$REMOTE_OUTPUT" | awk 'NF {print $1; exit}')" == "running" ]]; then
      record "$section" OK "Suricata process" "running"
    else
      record "$section" SKIP "Suricata process" "No running process reported" "Service is inactive or process is absent"
    fi
  else
    local rc=$?
    if (( rc == 125 )); then
      record "$section" SKIP "Suricata process" "SSH skipped" "Provide SSH access"
    else
      local detail
      detail="$(remote_failure_summary "No running process or suricata missing")"
      record "$section" WARN "Suricata process" "$host process check failed; ${detail}" "Verify IDS/IPS host and remote SSH/auth"
    fi
  fi
}

run_daily() {
  check_tcp "Доступность" "$DETMIR_SUPPORT_PVE_HOST" 22 "Proxmox SSH"
  check_tcp "Доступность" "$DETMIR_SUPPORT_PVE_HOST" 8006 "Proxmox WEB"
  check_tcp "Доступность" "$DETMIR_SUPPORT_AW_HOST" 5600 "AW API"
  check_tcp "Доступность" "$DETMIR_SUPPORT_WEB_HOST" 80 "Web HTTP"
  check_tcp "Доступность" "$DETMIR_SUPPORT_WEB_HOST" 443 "Web HTTPS"
  [[ -n "$DETMIR_SUPPORT_WINDOWS_HOST" ]] && check_tcp "Удаленный доступ" "$DETMIR_SUPPORT_WINDOWS_HOST" 3389 "Windows RDP"

  check_http "Ключевые сервисы" "http://$DETMIR_SUPPORT_AW_HOST:5600/api/0/settings/" '^2[0-9][0-9]$' "AW API health"
  check_http "Web" "https://$DETMIR_SUPPORT_WEB_TLS_HOST/healthz" '^200$' "Gateway healthz"
  check_http "Web" "https://$DETMIR_SUPPORT_WEB_TLS_HOST/" '^401$' "Gateway protected root"
  check_tls "TLS" "$DETMIR_SUPPORT_WEB_TLS_HOST" 443 "Web certificate"

  check_pfsense "Сеть"
  check_openvpn "Сеть"

  check_remote_service_status "Proxmox" "$DETMIR_SUPPORT_PVE_HOST" "$DETMIR_SUPPORT_PVE_SERVICES"
  check_remote_service_status "AW" "$DETMIR_SUPPORT_AW_HOST" "$DETMIR_SUPPORT_AW_SERVICES"
  check_remote_service_status "Web" "$DETMIR_SUPPORT_WEB_HOST" "$DETMIR_SUPPORT_WEB_SERVICES"

  check_node_load "Виртуализация" "$DETMIR_SUPPORT_PVE_HOST"
  check_disk "Диски" "$DETMIR_SUPPORT_PVE_HOST" "/"
  check_disk "Диски" "$DETMIR_SUPPORT_AW_HOST" "/"
  check_disk "Диски" "$DETMIR_SUPPORT_WEB_HOST" "/"

  check_backups "Резервные копии" "$DETMIR_SUPPORT_PVE_HOST" "$DETMIR_SUPPORT_PVE_BACKUP_DIRS"
  check_vms "Виртуализация" "$DETMIR_SUPPORT_PVE_HOST" "$DETMIR_SUPPORT_PVE_VM_IDS"
  check_task_log "Системные журналы" "$DETMIR_SUPPORT_PVE_HOST"
  check_suricata_service "Периметр" "$DETMIR_SUPPORT_SURICATA_HOST"
}

run_weekly() {
  run_daily

  check_log_growth "Логи" "$DETMIR_SUPPORT_PVE_HOST"
  check_log_growth "Логи" "$DETMIR_SUPPORT_AW_HOST"
  check_log_growth "Логи" "$DETMIR_SUPPORT_WEB_HOST"

  if run_remote "$DETMIR_SUPPORT_AW_HOST" "journalctl -p 3 --no-pager -n 40"; then
    local lines
    lines="$(printf '%s\n' "$REMOTE_OUTPUT" | wc -l)"
    if (( lines > 20 )); then
      record "Логи" "journalctl critical" WARN "Found ${lines} critical lines in recent entries" "Review and file ticket if recurring"
    else
      record "Логи" "journalctl critical" OK "Critical entries in normal range" ""
    fi
  else
    local rc=$?
    if (( rc == 125 )); then
      record "Логи" "journalctl critical" SKIP "SSH skipped" "Provide SSH access"
    else
      record "Логи" "journalctl critical" WARN "Unable to read journal" "Inspect journald"
    fi
  fi
}

run_monthly() {
  run_weekly

  local update_cmd
  # shellcheck disable=SC2016
  update_cmd=$(
    cat <<'EOF'
if command -v apt >/dev/null 2>&1; then
  apt list --upgradable 2>/dev/null | tail -n +2 | wc -l
elif command -v yum >/dev/null 2>&1; then
  yum check-update -q >/tmp/yumcheck 2>&1
  rc=$?
  if [ "$rc" -eq 100 ]; then
    wc -l < /tmp/yumcheck
  elif [ "$rc" -eq 0 ]; then
    echo 0
  else
    echo FAILED:$rc
  fi
else
  echo 0
fi
EOF
  )

  if run_remote "$DETMIR_SUPPORT_AW_HOST" "$update_cmd"; then
    if [[ "$REMOTE_OUTPUT" =~ ^[0-9]+$ ]]; then
      if (( REMOTE_OUTPUT > 15 )); then
        record "Обновления" "Update count" "${REMOTE_OUTPUT} updates available" "Согласуйте окно обслуживания"
      else
        record "Обновления" "Update count" "${REMOTE_OUTPUT} updates available" ""
      fi
    else
      record "Обновления" "Update check" "Unable parse update count" "Inspect package manager"
    fi
  else
    local rc=$?
    if (( rc == 125 )); then
      record "Обновления" "Update check" "SSH skipped" "Provide SSH access"
    else
      record "Обновления" "Update check" "Failed" "Inspect package manager/permissions"
    fi
  fi

  local pve_version
  if run_remote "$DETMIR_SUPPORT_PVE_HOST" "pveversion -v | head -n 1"; then
    pve_version="$REMOTE_OUTPUT"
    record "Контур" "Proxmox version" "${pve_version}" ""
  else
    local rc=$?
    if (( rc == 125 )); then
      record "Контур" "Proxmox version" "SSH skipped" "Provide SSH access"
    else
      record "Контур" "Proxmox version" "Unable to read" "Check pve command"
    fi
  fi

  record "Документация" OK "Access matrix" "Owner-visible matrix and contacts are required to be current" "Create/update support ownership map and include in evidence"
  record "DR" OK "Recovery drill" "Plan restore drill in maintenance window" "Schedule controlled restore drill on non-production clone"
}

generate_report() {
  local exit_code="$1"
  local status_text="OK"
  if (( exit_code != 0 )); then
    status_text="ATTENTION"
  fi

  {
    echo "# Отчет по выполнению задач поддержки DetMir"
    echo
    echo "- **Дата:** $(date -u '+%Y-%m-%d %H:%M:%SZ')"
    echo "- **Режим:** $SCOPE"
    echo "- **Run-ID:** $RUN_ID"
    echo "- **Лог:** $LOG_FILE"
    echo "- **Итоговый статус:** $status_text"
    echo
    echo "## Итого"
    echo
    echo "- OK: $OK_COUNT"
    echo "- WARN: $WARN_COUNT"
    echo "- FAIL: $FAIL_COUNT"
    echo "- SKIP: $SKIP_COUNT"
    echo "- TOTAL: $TOTAL"
    echo
    echo "## Детали"
    echo
    echo "|Раздел|Проверка|Статус|Результат|Действие|"
    echo "|---|---|---|---|---|"
    tail -n +2 "$CSV_FILE" | while IFS=';' read -r _ _ section status check result action; do
      section="${section//|/\\|}"
      check="${check//|/\\|}"
      status="${status//|/\\|}"
      result="${result//|/\\|}"
      action="${action//|/\\|}"
      echo "|$section|$check|$status|$result|$action|"
    done
    echo
  } > "$MD_FILE"

  cat <<EOF_SUMMARY

==> Отчет сохранен:
  - CSV: $CSV_FILE
  - Markdown: $MD_FILE
  - Log: $LOG_FILE
EOF_SUMMARY
}

main() {
  require_command bash || exit 1
  require_command awk || exit 1
  require_command date || exit 1
  require_command timeout || exit 1
  require_command curl || exit 1

  log "Start DetMir support run, scope=$SCOPE"
  log "Run directory: $RUN_DIR"

  case "$SCOPE" in
    daily)   run_daily ;;
    weekly)  run_weekly ;;
    monthly) run_monthly ;;
  esac

  local exit_code=0
  if (( FAIL_COUNT > 0 )); then
    exit_code=2
  elif (( WARN_COUNT > 0 )); then
    exit_code=1
  fi

  generate_report "$exit_code"
  log "Finish DetMir support run, status=$exit_code"
  return "$exit_code"
}

main "$@"
