#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="repo"
AW_API="${AW_API:-http://127.0.0.1:5600}"
AW_WORKTIME_API="${AW_WORKTIME_API:-http://127.0.0.1:5610}"
AW_LOGICAL_HOST_ID="${AW_LOGICAL_HOST_ID:-${AW_MONITORED_WINDOWS_HOSTNAME:-SHARKON2025}}"
AW_READINESS_TIMEOUT_SECONDS="${AW_READINESS_TIMEOUT_SECONDS:-60}"
AW_READINESS_INTERVAL_SECONDS="${AW_READINESS_INTERVAL_SECONDS:-2}"
AW_EVENTS_LIMIT="${AW_EVENTS_LIMIT:-100}"
AW_EVENTS_MAX_SECONDS="${AW_EVENTS_MAX_SECONDS:-15}"
HAYA_ROOT="${AW_HAYABUSA_ROOT:-/opt/hayabusa}"
HAYA_DROP_DIR="${AW_HAYABUSA_DROP_DIR:-/opt/activitywatch/aw-rus-ops/drop}"
SQLITE_DB="${AW_SQLITE_DB:-/var/lib/activitywatch/aw-server-rust/sqlite.db}"
MAX_INCOMING_AGE_SECONDS="${MAX_HAYABUSA_INCOMING_AGE_SECONDS:-900}"
STRICT_SECRETS="${DETMIR_RESILIENCE_STRICT_SECRETS:-0}"
EXPECT_DLP_PROFILE="${DETMIR_RESILIENCE_EXPECT_DLP_PROFILE:-light}"
EXPECT_OPTIONAL_DLP_OFF="${DETMIR_RESILIENCE_EXPECT_OPTIONAL_DLP_OFF:-0}"
EXPECT_LOKI_OFF="${DETMIR_RESILIENCE_EXPECT_LOKI_OFF:-1}"

DLP_RUNTIME_UNITS=(
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

DLP_LIGHT_ALLOWED_UNITS=(
  activitywatch-dlp-aggregator.timer
  activitywatch-dlp-aggregator.service
  aw-dlp-ioc-refresh.timer
  aw-dlp-ioc-refresh.service
  detmir-dlp-load-guard.timer
  detmir-dlp-load-guard.service
)

DLP_HEAVY_RUNTIME_UNITS=(
  aw-dlp-influx-exporter.timer
  aw-dlp-influx-exporter.service
  aw-dlp-report-scheduler.timer
  aw-dlp-report-scheduler.service
  aw-dlp-syslog-forwarder.timer
  aw-dlp-syslog-forwarder.service
  aw-dlp-webhook-sender.timer
  aw-dlp-webhook-sender.service
  aw-dlp-cef-exporter.timer
  aw-dlp-cef-exporter.service
  aw-dlp-policy-engine.service
  aw-dlp-case-management.service
  detmir-portal-evidence.service
)

LOKI_RUNTIME_UNITS=(
  loki.service
  promtail.service
)

OK_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0

usage() {
  cat <<'EOF'
Usage:
  scripts/detmir_resilience_check.sh [--repo|--live|--all]

Modes:
  --repo  check repository hardening and docs only (default, CI-safe)
  --live  read-only checks for the local AW server/Hayabusa host
  --all   repo + live

Environment:
  AW_API=http://127.0.0.1:5600
  AW_WORKTIME_API=http://127.0.0.1:5610
  AW_LOGICAL_HOST_ID=SHARKON2025
  AW_READINESS_TIMEOUT_SECONDS=60
  AW_EVENTS_MAX_SECONDS=15
  AW_HAYABUSA_ROOT=/opt/hayabusa
  AW_HAYABUSA_DROP_DIR=/opt/activitywatch/aw-rus-ops/drop
  AW_SQLITE_DB=/var/lib/activitywatch/aw-server-rust/sqlite.db
  DETMIR_RESILIENCE_EXPECT_DLP_PROFILE=light
  DETMIR_RESILIENCE_EXPECT_OPTIONAL_DLP_OFF=0
  DETMIR_RESILIENCE_EXPECT_LOKI_OFF=1
  DETMIR_RESILIENCE_STRICT_SECRETS=1
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo) MODE="repo"; shift ;;
    --live) MODE="live"; shift ;;
    --all) MODE="all"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

ok() {
  OK_COUNT=$((OK_COUNT + 1))
  printf '[OK]   %s\n' "$*"
}

warn() {
  WARN_COUNT=$((WARN_COUNT + 1))
  printf '[WARN] %s\n' "$*"
}

fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '[FAIL] %s\n' "$*"
}

have() {
  command -v "$1" >/dev/null 2>&1
}

require_file() {
  local path="$1"
  if [[ -f "$ROOT_DIR/$path" ]]; then
    ok "file exists: $path"
  else
    fail "missing file: $path"
  fi
}

require_pattern() {
  local path="$1"
  local pattern="$2"
  local label="$3"
  if grep -Eq "$pattern" "$ROOT_DIR/$path"; then
    ok "$label"
  else
    fail "$label"
  fi
}

check_repo() {
  printf '== repo resilience checks ==\n'

  require_file "scripts/detmir_resilience_check.sh"
  require_file "docs/DETMIR_RESILIENCE_HARDENING_RU.md"
  require_file "docs/DLP_RESOURCE_PROFILES_RU.md"
  require_file "docs/DLP_OPTIONAL_RUNTIME_RU.md"
  require_file "scripts/detmir_dlp_load_guard.sh"
  require_file "scripts/detmir_dlp_warehouse_sync.sh"
  require_file "aw-server/hayabusa/aw-hayabusa.sh"
  require_file "adk-rust/crates/hayabusa-tools/src/bin/autoprocess.rs"
  require_file "windows/AWatchRusCollectorGuardService.cs"
  require_file "windows/install-collector-guard-service.ps1"

  require_pattern "aw-server/hayabusa/aw-hayabusa.sh" "HAYA_QUARANTINE_DIR" "Hayabusa wrapper has quarantine root"
  require_pattern "aw-server/hayabusa/aw-hayabusa.sh" "quarantine_incoming_package" "Hayabusa wrapper isolates incoming poison packages"
  require_pattern "adk-rust/crates/hayabusa-tools/src/bin/autoprocess.rs" "validate_drop_inputs" "Hayabusa autoprocess validates drop package before accept"
  require_pattern "adk-rust/crates/hayabusa-tools/src/bin/autoprocess.rs" "quarantine_drop_package" "Hayabusa autoprocess quarantines bad drop package"
  require_pattern "windows/AWatchRusCollectorGuardService.cs" "Process\\.Exited|ChildExited" "Collector guard service watches child exit"
  require_pattern "windows/AWatchRusCollectorGuardService.cs" "MaxChildRestartsInWindow" "Collector guard service has bounded child restart budget"
  require_pattern "windows/install-collector-guard-service.ps1" "failureflag" "Collector guard installer enables SCM failureflag"
  require_pattern "docs/DETMIR_RESILIENCE_HARDENING_RU.md" "Hayabusa poison-package isolation" "Resilience doc records Hayabusa hardening"
  require_pattern "docs/DETMIR_RESILIENCE_HARDENING_RU.md" "Windows collector guard service child watchdog" "Resilience doc records guard child watchdog"
  require_pattern "docs/DLP_RESOURCE_PROFILES_RU.md" "core_only" "DLP resource profiles document core_only"
  require_pattern "docs/DLP_RESOURCE_PROFILES_RU.md" "auto.?disable|автоотключ" "DLP resource profiles document auto-disable guard"
  require_pattern "docs/DLP_RESOURCE_PROFILES_RU.md" "rollback" "DLP resource profiles document rollback"
  require_pattern "docs/DETMIR_CURRENT_STATE_RU.md" "AW_DLP_PROFILE=light" "Current state records DLP light profile"
  require_pattern "scripts/detmir_dlp_runtime_control.sh" "set-profile" "DLP runtime control supports profile switching"
  require_pattern "scripts/detmir_dlp_runtime_control.sh" "rollback_dlp" "DLP runtime control supports rollback"
  require_pattern "scripts/detmir_dlp_load_guard.sh" "set-profile core_only" "DLP load guard can auto-disable DLP to core_only"
  require_pattern "scripts/detmir_dlp_load_guard.sh" "STRIKES_REQUIRED" "DLP load guard requires consecutive overload checks"
  require_pattern "scripts/detmir_dlp_warehouse_sync.sh" "sqlite3 .*\\.backup" "DLP warehouse sync uses SQLite backup"
  require_pattern "ansible/group_vars/all.yml" 'aw_dlp_profile: "light"' "Production defaults keep DLP profile light"
  require_pattern "ansible/group_vars/all.yml" 'aw_dlp_enabled: true' "Production defaults enable lightweight DLP"
  require_pattern "ansible/group_vars/all.yml" 'aw_dlp_influx_enabled: false' "Production defaults keep DLP Influx disabled"
  require_pattern "ansible/group_vars/all.yml" 'aw_dlp_light_collector_enabled: true' "Production defaults enable lightweight DLP collector"
  require_pattern "ansible/group_vars/all.yml" 'aw_dlp_light_guard_enabled: true' "Production defaults enable DLP load guard"
  require_pattern "ansible/group_vars/all.yml" 'detmir_portal_dlp_module_enabled_override: true' "Production defaults expose DLP light status to portal"
  require_pattern "ansible/deploy_aw_server.yml" "detmir-dlp-load-guard.service" "AW server deploy installs DLP load guard service"
  require_pattern "ansible/deploy_aw_server.yml" "CPUQuota=.*aw_dlp_aggregator_cpu_quota" "DLP aggregator has systemd CPU quota"
  require_pattern "ansible/deploy_detmir_portal.yml" "detmir-dlp-warehouse-sync.service" "Portal deploy installs DLP warehouse sync service"
  require_pattern "ansible/deploy_detmir_portal.yml" "detmir_portal_dlp_module_enabled_override \\| default\\(false\\)" "Portal deploy defaults DLP module to disabled"

  if [[ -f "$ROOT_DIR/ansible/inventory.ini" ]] && grep -Eq '(^|[[:space:]])ansible_(become_)?password[[:space:]]*=[[:space:]]*[^<{]' "$ROOT_DIR/ansible/inventory.ini"; then
    if [[ "$STRICT_SECRETS" == "1" ]]; then
      fail "ansible/inventory.ini appears to contain literal password assignments; move them to vault/env"
    else
      warn "ansible/inventory.ini appears to contain literal password assignments; strict mode would fail"
    fi
  else
    ok "no literal ansible password assignments detected in ansible/inventory.ini"
  fi
}

check_systemd_unit() {
  local unit="$1"
  if ! have systemctl; then
    warn "systemctl unavailable; skipping $unit"
    return
  fi
  if systemctl is-active --quiet "$unit"; then
    ok "systemd active: $unit"
  else
    fail "systemd not active: $unit"
  fi
}

check_aw_api() {
  if ! have curl; then
    warn "curl unavailable; skipping AW API check"
    return
  fi
  local deadline last_code elapsed
  deadline=$((SECONDS + AW_READINESS_TIMEOUT_SECONDS))
  last_code=""
  while (( SECONDS <= deadline )); do
    elapsed="$(
      curl -sS --connect-timeout 3 --max-time 8 -o /dev/null -w '%{http_code} %{time_total}' \
        "$AW_API/api/0/info" 2>/dev/null || true
    )"
    last_code="${elapsed%% *}"
    if [[ "$last_code" == "200" ]]; then
      ok "ActivityWatch API readiness /api/0/info returns 200 (${elapsed#* }s)"
      return
    fi
    sleep "$AW_READINESS_INTERVAL_SECONDS"
  done
  case "$last_code" in
    503) fail "ActivityWatch API readiness ended on 503; possible datastore lock poisoning" ;;
    ""|000) fail "ActivityWatch API did not become ready within ${AW_READINESS_TIMEOUT_SECONDS}s" ;;
    *) fail "ActivityWatch API readiness unexpected final HTTP status: $last_code" ;;
  esac
}

check_aw_hot_path() {
  if ! have curl; then
    warn "curl unavailable; skipping AW hot-path event check"
    return
  fi
  local url result code elapsed
  url="${AW_API%/}/api/0/buckets/aw-worktime-sessions_${AW_LOGICAL_HOST_ID}/events?limit=${AW_EVENTS_LIMIT}"
  result="$(curl -sS --connect-timeout 3 --max-time "$AW_EVENTS_MAX_SECONDS" -o /dev/null -w '%{http_code} %{time_total}' "$url" 2>/dev/null || true)"
  code="${result%% *}"
  elapsed="${result#* }"
  if [[ "$code" != "200" ]]; then
    fail "ActivityWatch worktime events hot path returned HTTP ${code:-none}"
    return
  fi
  if awk -v t="$elapsed" -v max="$AW_EVENTS_MAX_SECONDS" 'BEGIN { exit !(t <= max) }'; then
    ok "ActivityWatch worktime events hot path returns 200 (${elapsed}s, limit=${AW_EVENTS_LIMIT})"
  else
    fail "ActivityWatch worktime events hot path exceeded ${AW_EVENTS_MAX_SECONDS}s (${elapsed}s)"
  fi
}

check_worktime_api() {
  if ! have curl; then
    warn "curl unavailable; skipping Worktime API check"
    return
  fi
  local tmp code
  tmp="$(mktemp)"
  code="$(curl -sS --connect-timeout 3 --max-time 12 -o "$tmp" -w '%{http_code}' \
    "${AW_WORKTIME_API%/}/reports/worktime/today?format=json&host=${AW_LOGICAL_HOST_ID}&allow_stale=1" 2>/dev/null || true)"
  if [[ "$code" != "200" ]]; then
    fail "Worktime API today report returned HTTP ${code:-none}"
    rm -f "$tmp"
    return
  fi
  if have jq; then
    if jq -e '(.degraded // false) == false' "$tmp" >/dev/null 2>&1; then
      ok "Worktime API today report is not degraded"
    else
      fail "Worktime API today report is degraded"
    fi
    if jq -e '((.rows // .users // []) | length) > 0' "$tmp" >/dev/null 2>&1; then
      ok "Worktime API today report has employee rows"
    else
      fail "Worktime API today report has no employee rows"
    fi
  else
    ok "Worktime API today report returns 200"
  fi
  rm -f "$tmp"
}

check_failed_units() {
  if ! have systemctl; then
    return
  fi
  local failed
  failed="$(systemctl --failed --no-legend --plain 2>/dev/null | wc -l | tr -d ' ')"
  if [[ "$failed" == "0" ]]; then
    ok "systemd failed units count is 0"
  else
    fail "systemd failed units count is $failed"
  fi
}

count_files() {
  local dir="$1"
  local pattern="$2"
  if [[ ! -d "$dir" ]]; then
    printf '0\n'
    return
  fi
  find "$dir" -maxdepth 1 -type f -name "$pattern" | wc -l | tr -d ' '
}

check_hayabusa_queues() {
  local incoming_dir="$HAYA_ROOT/inbox/incoming"
  local quarantine_dir="$HAYA_ROOT/quarantine"
  local incoming_count drop_count old_count quarantine_count
  incoming_count="$(count_files "$incoming_dir" '*.zip')"
  drop_count="$(count_files "$HAYA_DROP_DIR" '*.zip')"
  old_count="0"
  if [[ -d "$incoming_dir" ]]; then
    old_count="$(find "$incoming_dir" -maxdepth 1 -type f -name '*.zip' -mmin "+$((MAX_INCOMING_AGE_SECONDS / 60))" | wc -l | tr -d ' ')"
  fi
  if [[ "$incoming_count" == "0" ]]; then
    ok "Hayabusa incoming zip count is 0"
  else
    warn "Hayabusa incoming zip count is $incoming_count"
  fi
  if [[ "$drop_count" == "0" ]]; then
    ok "Hayabusa drop zip count is 0"
  else
    warn "Hayabusa drop zip count is $drop_count"
  fi
  if [[ "$old_count" == "0" ]]; then
    ok "Hayabusa incoming has no stale zip older than ${MAX_INCOMING_AGE_SECONDS}s"
  else
    fail "Hayabusa incoming has $old_count stale zip package(s)"
  fi
  if [[ -d "$quarantine_dir" ]]; then
    quarantine_count="$(find "$quarantine_dir" -type f -name reason.json | wc -l | tr -d ' ')"
    if [[ "$quarantine_count" == "0" ]]; then
      ok "Hayabusa quarantine reason count is 0"
    else
      warn "Hayabusa quarantine reason count is $quarantine_count; review/replay policy required"
    fi
  else
    warn "Hayabusa quarantine root not found yet: $quarantine_dir"
  fi
}

check_sqlite_files() {
  if [[ ! -f "$SQLITE_DB" ]]; then
    warn "AW SQLite DB not found at $SQLITE_DB; skipping local DB size check"
    return
  fi
  local db_size wal_size
  db_size="$(stat -c '%s' "$SQLITE_DB")"
  wal_size="0"
  [[ -f "$SQLITE_DB-wal" ]] && wal_size="$(stat -c '%s' "$SQLITE_DB-wal")"
  if (( db_size > 5 * 1024 * 1024 * 1024 )); then
    fail "AW SQLite DB exceeds 5GiB"
  elif (( db_size > 2 * 1024 * 1024 * 1024 )); then
    warn "AW SQLite DB exceeds 2GiB"
  else
    ok "AW SQLite DB size below 2GiB"
  fi
  if (( wal_size > 1024 * 1024 * 1024 )); then
    fail "AW SQLite WAL exceeds 1GiB"
  elif (( wal_size > 256 * 1024 * 1024 )); then
    warn "AW SQLite WAL exceeds 256MiB"
  else
    ok "AW SQLite WAL size below 256MiB"
  fi
}

check_sqlite_hot_path_index() {
  if [[ ! -f "$SQLITE_DB" ]]; then
    warn "AW SQLite DB not found at $SQLITE_DB; skipping hot-path index check"
    return
  fi
  if ! have sqlite3; then
    warn "sqlite3 unavailable; skipping hot-path index check"
    return
  fi
  local index_exists plan
  index_exists="$(sqlite3 "$SQLITE_DB" "SELECT name FROM sqlite_master WHERE type='index' AND name='events_bucketrow_starttime_desc_index';" 2>/dev/null || true)"
  if [[ "$index_exists" == "events_bucketrow_starttime_desc_index" ]]; then
    ok "AW SQLite hot-path index exists"
  else
    fail "AW SQLite hot-path index events_bucketrow_starttime_desc_index is missing"
    return
  fi
  plan="$(
    sqlite3 "$SQLITE_DB" "EXPLAIN QUERY PLAN SELECT id,starttime,endtime,data FROM events WHERE bucketrow=(SELECT id FROM buckets WHERE name='aw-worktime-sessions_${AW_LOGICAL_HOST_ID}') ORDER BY starttime DESC LIMIT ${AW_EVENTS_LIMIT};" 2>/dev/null || true
  )"
  if printf '%s' "$plan" | grep -q 'events_bucketrow_starttime_desc_index'; then
    ok "AW SQLite worktime event query uses hot-path index"
  else
    fail "AW SQLite worktime event query does not use hot-path index"
  fi
  if printf '%s' "$plan" | grep -q 'TEMP B-TREE'; then
    fail "AW SQLite worktime event query still builds TEMP B-TREE"
  else
    ok "AW SQLite worktime event query avoids TEMP B-TREE"
  fi
}

check_units_inactive() {
  local label="$1"
  shift
  if ! have systemctl; then
    warn "systemctl unavailable; skipping $label runtime check"
    return
  fi
  local active_units=()
  local unit active
  for unit in "$@"; do
    active="$(systemctl is-active "$unit" 2>/dev/null || true)"
    if [[ "$active" == "active" || "$active" == "activating" ]]; then
      active_units+=("$unit:$active")
    fi
  done
  if [[ "${#active_units[@]}" -eq 0 ]]; then
    ok "$label runtime units are inactive"
  else
    fail "$label runtime units active: ${active_units[*]}"
  fi
}

check_live() {
  printf '== live resilience checks ==\n'
  check_systemd_unit "activitywatch-server"
  check_systemd_unit "aw-worktime-api"
  check_aw_api
  check_aw_hot_path
  check_worktime_api
  check_failed_units
  check_hayabusa_queues
  check_sqlite_files
  check_sqlite_hot_path_index
  if [[ "$EXPECT_OPTIONAL_DLP_OFF" == "1" || "$EXPECT_DLP_PROFILE" == "core_only" ]]; then
    check_units_inactive "optional DLP" "${DLP_RUNTIME_UNITS[@]}"
  elif [[ "$EXPECT_DLP_PROFILE" == "light" ]]; then
    check_units_inactive "heavy DLP" "${DLP_HEAVY_RUNTIME_UNITS[@]}"
  else
    warn "optional DLP runtime inactive check skipped for profile=$EXPECT_DLP_PROFILE"
  fi
  if [[ "$EXPECT_LOKI_OFF" == "1" ]]; then
    check_units_inactive "Loki" "${LOKI_RUNTIME_UNITS[@]}"
  else
    warn "Loki inactive check skipped by env"
  fi
}

case "$MODE" in
  repo) check_repo ;;
  live) check_live ;;
  all) check_repo; check_live ;;
  *) fail "invalid mode: $MODE" ;;
esac

printf 'summary: ok=%s warn=%s fail=%s\n' "$OK_COUNT" "$WARN_COUNT" "$FAIL_COUNT"
if (( FAIL_COUNT > 0 )); then
  exit 1
fi
