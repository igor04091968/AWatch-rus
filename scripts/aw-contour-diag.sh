#!/usr/bin/env bash
# aw-contour-diag.sh - Диагностика всего контура ActivityWatch-Russian
# Запускать с машины администратора (где есть доступ по SSH/curl ко всем узлам).
#
# Использование:
#   ./scripts/aw-contour-diag.sh                  # полная диагностика
#   ./scripts/aw-contour-diag.sh --quick           # быстрая (только AW server + buckets)
#   ./scripts/aw-contour-diag.sh --skip-windows    # без RDP/WinRM проверок
#
# При красных проверках в скрипте указаны разделы 'REMEDIATION: ...'
# с конкретными командами для восстановления.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ANSIBLE_DIR="$REPO_ROOT/ansible"
INVENTORY="$ANSIBLE_DIR/inventory.ini"
AW_SERVER="http://10.10.10.13:5600"
AW_WORKTIME_API="http://10.10.10.13:5610"
INFLUXDB_URL="http://10.10.10.10:8086"
GRAFANA_URL="http://10.10.10.11:3000"
PROXMOX_HOST="10.10.10.2"
AW_HOST="10.10.10.13"
GRAFANA_HOST="10.10.10.11"
INFLUXDB_HOST="10.10.10.10"
WINDOWS_HOST="192.168.100.18"
CLICKHOUSE_HOST="10.10.10.2"
SOURCE_HOSTNAME="SHARKON2025"

QUICK_MODE=0
SKIP_WINDOWS=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick) QUICK_MODE=1; shift ;;
    --skip-windows) SKIP_WINDOWS=1; shift ;;
    -h|--help)
      echo "Usage: $(basename "$0") [--quick] [--skip-windows]"
      exit 0 ;;
    *) echo "Unknown: $1"; exit 2 ;;
  esac
done

export no_proxy="localhost,127.0.0.1,$PROXMOX_HOST,$AW_HOST,$GRAFANA_HOST,$INFLUXDB_HOST,$WINDOWS_HOST,$CLICKHOUSE_HOST,10.10.10.0/24,192.168.100.0/24"
export NO_PROXY="$no_proxy"

OK_COUNT=0; WARN_COUNT=0; FAIL_COUNT=0; SKIP_COUNT=0

if [ -t 1 ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; CYAN=''; NC=''
fi

pass()  { OK_COUNT=$((OK_COUNT+1)); printf "%b[OK]%b   %s\n"      "$GREEN" "$NC" "$*"; }
warn()  { WARN_COUNT=$((WARN_COUNT+1)); printf "%b[WARN]%b %s\n"  "$YELLOW" "$NC" "$*"; }
fail()  { FAIL_COUNT=$((FAIL_COUNT+1)); printf "%b[FAIL]%b %s\n"  "$RED" "$NC" "$*"; }
skip()  { SKIP_COUNT=$((SKIP_COUNT+1)); printf "%b[SKIP]%b %s\n"  "$YELLOW" "$NC" "$*"; }
section() { printf "\n%b=== %s ===%b\n" "$CYAN" "$*" "$NC"; }
have()  { command -v "$1" >/dev/null 2>&1; }

check_tcp() {
  local name="$1" host="$2" port="$3"
  if timeout 4 bash -c ":</dev/tcp/${host}/${port}" >/dev/null 2>&1; then
    pass "TCP $host:$port ($name)"
  else
    fail "TCP $host:$port ($name)"
    echo "       REMEDIATION: Проверьте, запущен ли сервис на $host:$port."
    echo "       Для systemd: ssh igor@$host 'systemctl status <unit>'"
    echo "       Для Docker:  ssh igor@$PROXMOX_HOST 'sudo docker ps | grep <container>'"
  fi
}

check_http_code() {
  local name="$1" url="$2" expected="${3:-^2[0-9][0-9]$}"
  local tmp code
  tmp="$(mktemp)"
  code="$(curl -k -sS --connect-timeout 5 --max-time 15 -o "$tmp" -w '%{http_code}' "$url" 2>"$tmp.err")"
  if printf "%s" "$code" | grep -Eq "$expected"; then
    pass "HTTP $code $url ($name)"
  else
    fail "HTTP $code $url ($name)"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -20
  fi
  rm -f "$tmp" "$tmp.err"
}

check_http_json_key() {
  local name="$1" url="$2" jq_filter="$3" remediation="$4"
  local tmp
  tmp="$(mktemp)"
  if curl -k -fsS --connect-timeout 5 --max-time 20 "$url" -o "$tmp" 2>"$tmp.err" && jq -e "$jq_filter" "$tmp" >/dev/null 2>&1; then
    pass "$name"
  else
    fail "$name"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -10
    echo "       REMEDIATION: $remediation"
  fi
  rm -f "$tmp" "$tmp.err"
}

check_bucket_freshness() {
  local bucket="$1" label="$2" remediation="$3"
  local bucket_id="${bucket}_${SOURCE_HOSTNAME}"
  local tmp last_ts event_epoch now age_sec
  tmp="$(mktemp)"
  if ! curl -fsS --connect-timeout 5 --max-time 15 "$AW_SERVER/api/0/buckets/$bucket_id/events?limit=1" -o "$tmp" 2>"$tmp.err"; then
    fail "bucket $label ($bucket_id) — запрос не удался"
    sed 's/^/       /' "$tmp.err" | head -5
    echo "       REMEDIATION: $remediation"
    rm -f "$tmp" "$tmp.err"
    return
  fi
  last_ts="$(jq -r '.[0].timestamp // empty' "$tmp" 2>/dev/null)"
  rm -f "$tmp"
  if [ -z "$last_ts" ]; then
    warn "bucket $label ($bucket_id) — нет событий"
    echo "       REMEDIATION: $remediation"
    return
  fi
  event_epoch="$(date -d "$last_ts" +%s 2>/dev/null || echo 0)"
  now="$(date -u +%s)"
  age_sec=$((now - event_epoch))

  case "$bucket" in
    aw-dlp-incidents|aw-dlp-review|aw-dlp-rules|aw-session-events)
      if [ "$age_sec" -lt 86400 ]; then
        pass "bucket $label — ${age_sec}s назад"
      else
        warn "bucket $label — ${age_sec}s назад (event-driven)"
      fi ;;
    aw-watcher-window|aw-dlp-endpoint-signals)
      if [ "$age_sec" -lt 7200 ]; then
        pass "bucket $label — ${age_sec}s назад"
      else
        warn "bucket $label — ${age_sec}s назад (INACTIVE)"
      fi ;;
    *)
      if [ "$age_sec" -lt 3600 ]; then
        pass "bucket $label — ${age_sec}s назад"
      elif [ "$age_sec" -lt 86400 ]; then
        warn "bucket $label — ${age_sec}s назад (STALE)"
        echo "       REMEDIATION: $remediation"
      else
        fail "bucket $label — ${age_sec}s назад (DEAD)"
        echo "       REMEDIATION: $remediation"
      fi ;;
  esac
}

check_ansible_shell() {
  local name="$1" group="$2" command="$3"
  if ! have ansible; then
    skip "$name (ansible not available)"
    return
  fi
  if [ ! -f "$INVENTORY" ]; then
    skip "$name (inventory not found: $INVENTORY)"
    return
  fi
  local tmp
  tmp="$(mktemp)"
  if ANSIBLE_NOCOLOR=1 ansible "$group" -i "$INVENTORY" -m shell -a "$command" >"$tmp" 2>&1; then
    pass "$name"
  else
    fail "$name"
    sed 's/^/       /' "$tmp" | head -20
  fi
  rm -f "$tmp"
}

check_ansible_win_shell() {
  local name="$1" command="$2"
  if ! have ansible; then skip "$name (ansible not available)"; return; fi
  if [ ! -f "$INVENTORY" ]; then skip "$name (inventory not found)"; return; fi
  local tmp
  tmp="$(mktemp)"
  if ANSIBLE_NOCOLOR=1 ansible aw_windows -i "$INVENTORY" -m win_shell -a "$command" >"$tmp" 2>&1; then
    pass "$name"
  else
    fail "$name"
    sed 's/^/       /' "$tmp" | head -20
  fi
  rm -f "$tmp"
}

check_ansible_module() {
  local name="$1" group="$2" module="$3" args="${4:-}"
  if ! have ansible; then skip "$name (ansible not available)"; return; fi
  if [ ! -f "$INVENTORY" ]; then skip "$name (inventory not found)"; return; fi
  local tmp
  tmp="$(mktemp)"
  if ANSIBLE_NOCOLOR=1 ansible "$group" -i "$INVENTORY" -m "$module" ${args:+-a "$args"} >"$tmp" 2>&1; then
    pass "$name"
  else
    fail "$name"
    sed 's/^/       /' "$tmp" | head -20
  fi
  rm -f "$tmp"
}

ssh_with_diag_password() {
  local host="$1"
  shift
  if [[ -z "${AW_DIAG_SSH_PASSWORD:-}" ]]; then
    return 125
  fi
  sshpass -p "$AW_DIAG_SSH_PASSWORD" ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no "igor@$host" "$@"
}

ssh_aw()  { ssh_with_diag_password 10.10.10.13 "$@"; }
ssh_pve() { ssh_with_diag_password 10.10.10.2 "$@"; }

check_service_remote() {
  local name="$1" host="$2" unit="$3" remediation="$4"
  local result
  result=$(ssh_with_diag_password "$host" "systemctl is-active $unit 2>/dev/null || echo not_found" 2>/dev/null)
  local rc=$?
  if [ "$rc" -eq 125 ]; then
    skip "$name ($unit on $host — set AW_DIAG_SSH_PASSWORD for SSH checks)"
    return
  fi
  if [ "$rc" -ne 0 ] || [ "$result" = "not_found" ]; then
    skip "$name ($unit on $host — не удалось проверить)"
    return
  fi
  if [ "$result" = "active" ]; then
    pass "$name ($unit active on $host)"
  else
    fail "$name ($unit $result on $host)"
    echo "       REMEDIATION: $remediation"
  fi
}

printf "%b=== ActivityWatch-Russian: Диагностика контура ===%b\n" "$CYAN" "$NC"
echo "       $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo ""

# ============================================================
section "1. Локальные предусловия"
# ============================================================
for cmd in bash curl jq timeout ssh sshpass; do
  if have "$cmd"; then pass "утилита $cmd найдена"; else fail "утилита $cmd не найдена (установите: apt install $cmd)"; fi
done
echo ""

# ============================================================
section "2. TCP доступность узлов"
# ============================================================
check_tcp "AW Server"              "$AW_HOST"        5600
check_tcp "Worktime API"           "$AW_HOST"        5610
check_tcp "RDP WinRM"              "$WINDOWS_HOST"   5985
check_tcp "Proxmox SSH"            "$PROXMOX_HOST"   22
check_tcp "Proxmox HTTPS"          "$PROXMOX_HOST"   443
check_tcp "1C Company API"         "$PROXMOX_HOST"   8710
check_tcp "ClickHouse HTTP"        "$CLICKHOUSE_HOST" 8123
check_tcp "ClickHouse Native"      "$CLICKHOUSE_HOST" 9000
check_tcp "InfluxDB"               "$INFLUXDB_HOST"  8086
check_tcp "Grafana"                "$GRAFANA_HOST"   3000

if [ "$QUICK_MODE" = "1" ]; then
  # В быстром режиме проверяем только AW Server и buckets
  echo ""
  section "3. AW Server (быстрый режим)"
  check_http_code "AW Server info" "$AW_SERVER/api/0/info" '^200$'
  check_http_code "AW Server CORS" "$AW_SERVER/api/0/settings/" '^200$'
  check_http_code "Worktime API health" "$AW_WORKTIME_API/health" '^200$'

  section "4. Buckets (быстрый режим)"
  for entry in \
    "aw-watcher-afk|AFK watcher|Запустите на RDP: schtasks /Run /TN \"ActivityWatch Recovery\" или schtasks /Run /TN \"ActivityWatch Launch [SHARKON2025_Администратор]\"" \
    "aw-watcher-window|Window watcher|Запустите через ansible: ansible aw_windows -i $INVENTORY -m win_shell -a 'Start-Process -FilePath \"C:\\Program Files\\AWatch-rus\\bin\\aw-watcher-window\\aw-watcher-window.exe\" -ArgumentList @(\"--host\", \"10.10.10.13\", \"--port\", \"5600\") -WindowStyle Hidden'" \
    "aw-worktime-sessions|Worktime sessions|Проверьте работу worktime-api: systemctl status aw-worktime-api на AW сервере" \
    "aw-session-events|Session events|Проверьте collector-guard и aw-session-events-collector на RDP" \
    "aw-dlp-endpoint-signals|DLP signals|Запустите: ansible aw_windows -i $INVENTORY -m win_shell -a 'schtasks /Run /TN \"ActivityWatch Launch [SHARKON2025_Администратор]\"; Start-Sleep 30'" \
    "aw-dlp-incidents|DLP incidents|Проверьте aw-detmir-dlp-collector.ps1 на RDP (логи: C:\\ProgramData\\AWatch-rus\\logs\\)" \
  ; do
    bucket="${entry%%|*}"; rest="${entry#*|}"
    label="${rest%%|*}"; remediation="${rest#*|}"
    check_bucket_freshness "$bucket" "$label" "$remediation"
  done
  echo ""
  echo "=== Быстрая диагностика завершена ==="
  printf "OK=%s WARN=%s FAIL=%s SKIP=%s\n" "$OK_COUNT" "$WARN_COUNT" "$FAIL_COUNT" "$SKIP_COUNT"
  [ "$FAIL_COUNT" -gt 0 ] && exit 2 || exit 0
fi

# ============================================================
section "3. AW Server (10.10.10.13)"
# ============================================================
check_http_json_key "AW Server info" "$AW_SERVER/api/0/info" \
  '.version' \
  "Проверьте: ssh igor@$AW_HOST 'systemctl status activitywatch-server'"
check_http_code "AW Server CORS" "$AW_SERVER/api/0/settings/" '^200$'
check_http_code "AW WebUI" "$AW_SERVER/" '^200$'
check_http_json_key "Worktime API health" "$AW_WORKTIME_API/health" \
  '.status // .ok' \
  "Проверьте: ssh igor@$AW_HOST 'systemctl status aw-worktime-api && journalctl -u aw-worktime-api -n 20'"

# ============================================================
section "4. Buckets (свежесть данных)"
# ============================================================
for entry in \
  "aw-watcher-afk|AFK watcher|Запустите на RDP: ansible aw_windows -i $INVENTORY -m win_shell -a 'schtasks /Run /TN \"ActivityWatch Recovery\"'" \
  "aw-watcher-window|Window watcher|Запустите: ansible aw_windows -i $INVENTORY -m win_shell -a 'schtasks /Run /TN \"ActivityWatch Launch [SHARKON2025_Администратор]\"; Start-Process -FilePath \"C:\\Program Files\\AWatch-rus\\bin\\aw-watcher-window\\aw-watcher-window.exe\" -ArgumentList @(\"--host\", \"10.10.10.13\", \"--port\", \"5600\") -WindowStyle Hidden'" \
  "aw-worktime-sessions|Worktime sessions|Проверьте: ssh igor@$AW_HOST 'systemctl status aw-worktime-api && journalctl -u aw-worktime-api -n 20'" \
  "aw-session-events|Session events|Проверьте collector-guard на RDP: ansible aw_windows -i $INVENTORY -m win_shell -a 'Get-Process -Name aw-session-events-* -ErrorAction SilentlyContinue'" \
  "aw-dlp-endpoint-signals|DLP endpoint signals|Запустите: ansible aw_windows -i $INVENTORY -m win_shell -a 'schtasks /Run /TN \"ActivityWatch Launch [SHARKON2025_Администратор]\"; Start-Sleep 60'" \
  "aw-dlp-incidents|DLP incidents|Проверьте: ansible aw_windows -i $INVENTORY -m win_shell -a \"Get-Content 'C:\\ProgramData\\AWatch-rus\\logs\\dlp-*.log' -Tail 20\"" \
  "aw-dlp-review|DLP review|Проверьте: ssh igor@$AW_HOST 'journalctl -u aw-dlp-policy-engine.service -n 20 --no-pager'" \
  "aw-dlp-rules|DLP rules|Проверьте: ssh igor@$AW_HOST 'journalctl -u aw-dlp-ioc-refresh.service -n 20 --no-pager'" \
; do
  bucket="${entry%%|*}"; rest="${entry#*|}"
  label="${rest%%|*}"; remediation="${rest#*|}"
  check_bucket_freshness "$bucket" "$label" "$remediation"
done

# ============================================================
section "5. InfluxDB (10.10.10.10:8086)"
# ============================================================
check_http_json_key "InfluxDB health" "$INFLUXDB_URL/health" \
  '.status == "pass"' \
  "Проверьте InfluxDB на LXC 200: ssh igor@$PROXMOX_HOST 'sudo pct exec 200 -- systemctl status influxdb'"

# ============================================================
section "6. Grafana (10.10.10.11:3000)"
# ============================================================
check_http_json_key "Grafana health" "$GRAFANA_URL/api/health" \
  '.database == "ok"' \
  "Проверьте: ssh igor@$PROXMOX_HOST 'sudo pct exec 201 -- systemctl status grafana-server'"
check_http_code "Grafana datasources API" "$GRAFANA_URL/api/datasources" '^200$|^302$|^401$'

# ============================================================
section "7. ClickHouse (10.10.10.2:8123)"
# ============================================================
# Проверяем через прямой HTTP — AUTHENTICATION_FAILED = сервер жив
local_ch_ok=0
ch_code=$(curl -sS --max-time 5 "http://$CLICKHOUSE_HOST:8123/?query=SELECT%201" 2>/dev/null | head -1)
if echo "$ch_code" | grep -q "AUTHENTICATION_FAILED"; then
  pass "ClickHouse HTTP — отвечает (требуется аутентификация, это нормально)"
  local_ch_ok=1
elif echo "$ch_code" | grep -q "1"; then
  pass "ClickHouse HTTP — SELECT 1 OK"
  local_ch_ok=1
else
  fail "ClickHouse HTTP — не отвечает: $ch_code"
  echo "       REMEDIATION: ssh igor@$PROXMOX_HOST 'cd /opt/activitywatch/clickhouse-1c && sudo docker compose ps; sudo docker compose logs --tail=20'"
fi

# Проверка Docker контейнера через SSH
container_status=$(ssh_pve 'sudo docker ps --filter name=aw-rus-1c-clickhouse --format "{{.Status}}" 2>/dev/null' 2>/dev/null)
if [ -n "$container_status" ]; then
  pass "ClickHouse Docker контейнер: $container_status"
else
  fail "ClickHouse Docker контейнер не запущен"
  echo "       REMEDIATION: ssh igor@$PROXMOX_HOST 'cd /opt/activitywatch/clickhouse-1c && sudo docker compose up -d'"
fi

# ClickHouse health timer
check_service_remote "ClickHouse health timer" "$PROXMOX_HOST" "aw-1c-clickhouse-health.timer" \
  "Проверьте: ssh igor@$PROXMOX_HOST 'sudo journalctl -u aw-1c-clickhouse-health.service -n 30 --no-pager'"

# ClickHouse network health timer (с AW сервера)
check_service_remote "ClickHouse network health timer" "$AW_HOST" "aw-clickhouse-network-health.timer" \
  "Проверьте: ssh igor@$AW_HOST 'sudo journalctl -u aw-clickhouse-network-health.service -n 30 --no-pager'"

# 1C-ingest timer
check_service_remote "1C ingest timer" "$PROXMOX_HOST" "aw-1c-ingest.timer" \
  "Проверьте: ssh igor@$PROXMOX_HOST 'sudo systemctl status aw-1c-ingest.timer; sudo journalctl -u aw-1c-ingest.service -n 20'"

# ============================================================
section "8. 1C Manager API (10.10.10.2:8710)"
# ============================================================
check_http_json_key "1C /api/health" "http://$PROXMOX_HOST:8710/api/health" \
  '.status == "ok"' \
  "Проверьте Python процесс: ssh igor@$PROXMOX_HOST 'ps aux | grep 8710 | grep -v grep'"
check_http_code "1C /manager/brief" "http://$PROXMOX_HOST:8710/manager/brief" '^200$'

# ============================================================
section "9. Nginx Gateway (10.10.10.2)"
# ============================================================
check_http_code "Gateway /healthz" "https://$PROXMOX_HOST/healthz" '^200$'
check_http_code "Gateway /go/proxmox-gui (401=protected, OK)" "https://$PROXMOX_HOST/go/proxmox-gui" '^30[1278]$|^401$'
check_http_code "Gateway /go/file1c-brief (401=protected, OK)" "https://$PROXMOX_HOST/go/file1c-brief" '^30[1278]$|^401$'

check_service_remote "Nginx service" "$PROXMOX_HOST" "nginx.service" \
  "Проверьте: ssh igor@$PROXMOX_HOST 'sudo systemctl status nginx; sudo nginx -t'"

# ============================================================
section "10. DLP Pipeline (10.10.10.13)"
# ============================================================
# DLP Policy Engine — должен быть active (running)
check_service_remote "DLP Policy Engine" "$AW_HOST" "aw-dlp-policy-engine.service" \
  "Проверьте: ssh igor@$AW_HOST 'sudo journalctl -u aw-dlp-policy-engine.service -n 30 --no-pager'"

# DLP Case Management
check_service_remote "DLP Case Management" "$AW_HOST" "aw-dlp-case-management.service" \
  "Проверьте: ssh igor@$AW_HOST 'sudo journalctl -u aw-dlp-case-management.service -n 30 --no-pager'"

# DLP Aggregator
aggr_status=$(ssh_aw 'systemctl is-active activitywatch-dlp-aggregator.timer 2>/dev/null || echo not_found' 2>/dev/null)
if [ "$aggr_status" = "active" ]; then
  pass "DLP Aggregator timer (active)"
else
  fail "DLP Aggregator timer ($aggr_status)"
  echo "       REMEDIATION: ssh igor@$AW_HOST 'sudo systemctl enable --now activitywatch-dlp-aggregator.timer; sudo journalctl -u activitywatch-dlp-aggregator.service -n 30'"
fi

# DLP Influx Exporter
influx_exp_status=$(ssh_aw 'systemctl is-active aw-dlp-influx-exporter.timer 2>/dev/null || echo not_found' 2>/dev/null)
if [ "$influx_exp_status" = "active" ]; then
  pass "DLP Influx Exporter timer (active)"
else
  fail "DLP Influx Exporter timer ($influx_exp_status)"
  echo "       REMEDIATION: ssh igor@$AW_HOST 'sudo systemctl enable --now aw-dlp-influx-exporter.timer; sudo journalctl -u aw-dlp-influx-exporter.service -n 30'"
fi

# DLP CEF Exporter
check_service_remote "DLP CEF Exporter timer" "$AW_HOST" "aw-dlp-cef-exporter.timer" \
  "ssh igor@$AW_HOST 'sudo systemctl enable --now aw-dlp-cef-exporter.timer; journalctl -u aw-dlp-cef-exporter.service -n 20'"

# DLP IOC Refresh
check_service_remote "DLP IOC Refresh timer" "$AW_HOST" "aw-dlp-ioc-refresh.timer" \
  "ssh igor@$AW_HOST 'sudo systemctl enable --now aw-dlp-ioc-refresh.timer'"

# DLP Syslog Forwarder
check_service_remote "DLP Syslog Forwarder timer" "$AW_HOST" "aw-dlp-syslog-forwarder.timer" \
  "ssh igor@$AW_HOST 'sudo systemctl enable --now aw-dlp-syslog-forwarder.timer'"

# DLP Webhook Sender
check_service_remote "DLP Webhook Sender timer" "$AW_HOST" "aw-dlp-webhook-sender.timer" \
  "ssh igor@$AW_HOST 'sudo systemctl enable --now aw-dlp-webhook-sender.timer'"

# DLP Report Scheduler
check_service_remote "DLP Report Scheduler timer" "$AW_HOST" "aw-dlp-report-scheduler.timer" \
  "ssh igor@$AW_HOST 'sudo systemctl enable --now aw-dlp-report-scheduler.timer'"

# Worktime Influx Exporter
check_service_remote "Worktime Influx Exporter timer" "$AW_HOST" "aw-worktime-influx-exporter.timer" \
  "ssh igor@$AW_HOST 'sudo systemctl enable --now aw-worktime-influx-exporter.timer; journalctl -u aw-worktime-influx-exporter.service -n 20'"

# ============================================================
if [ "$SKIP_WINDOWS" = "1" ]; then
  skip "Проверки RDP хоста пропущены (--skip-windows)"
else
  section "11. RDP хост (192.168.100.18)"
  if have ansible && [ -f "$INVENTORY" ]; then
    check_ansible_module "WinRM ping" aw_windows win_ping

    check_ansible_win_shell "Сессии RDP" 'query user 2>&1'

    check_ansible_win_shell "Процессы watcher" \
      'Get-Process aw-watcher-afk,aw-watcher-window -ErrorAction SilentlyContinue | Select-Object Name,Id,SessionId,StartTime | Format-Table -AutoSize'

    check_ansible_win_shell "Количество процессов powershell" \
      '(Get-Process powershell -ErrorAction SilentlyContinue | Measure-Object).Count'

    check_ansible_win_shell "Scheduled tasks (Recovery)" \
      'schtasks /Query /TN "ActivityWatch Recovery" /FO LIST /V | Select-String "Status|Run|Next"'

    check_ansible_win_shell "Scheduled tasks (Launch Admin)" \
      'schtasks /Query /TN "ActivityWatch Launch [SHARKON2025_Администратор]" /FO LIST /V | Select-String "Status|Run|Next"'
  else
    skip "Ansible или inventory не найдены"
  fi
fi

# ============================================================
section "12. Systemd health (AW server)"
# ============================================================
check_ansible_shell "AW server core units" aw_server \
  'systemctl is-active activitywatch-server aw-worktime-api aw-dlp-policy-engine aw-dlp-case-management aw-worktime-influx-exporter.timer aw-dlp-influx-exporter.timer activitywatch-dlp-aggregator.timer aw-clickhouse-network-health.timer | paste -sd,'

check_ansible_shell "AW server — нет failed units" aw_server \
  'failed=$(systemctl --failed --no-legend | awk "{print \$1}" | grep -E "activitywatch|aw-|dlp" || true); test -z "$failed" && echo "no AW-related failed units" || { echo "$failed"; exit 1; }'

# ============================================================
section "13. Systemd health (Proxmox)"
# ============================================================
check_ansible_shell "Proxmox core units" proxmox \
  'systemctl is-active nginx docker aw-1c-clickhouse-health.timer aw-1c-ingest.timer 2>/dev/null | paste -sd,'

# ============================================================
section "14. Диск и память"
# ============================================================
check_ansible_shell "Диски AW server" aw_server 'df -h / /var /opt 2>/dev/null | tail -5'
check_ansible_shell "Память AW server" aw_server 'free -h | tail -5'

# ============================================================
section "Итог диагностики"
# ============================================================
printf "  OK=%s  WARN=%s  FAIL=%s  SKIP=%s\n" "$OK_COUNT" "$WARN_COUNT" "$FAIL_COUNT" "$SKIP_COUNT"

if [ "$FAIL_COUNT" -gt 0 ]; then
  echo ""
  echo "  Есть проблемы! Смотрите REMEDIATION выше для каждого FAIL."
  echo "  После исправления запустите повторно: $0"
  exit 2
elif [ "$WARN_COUNT" -gt 0 ]; then
  echo ""
  echo "  Есть предупреждения (WARN) — стоит проверить, но не критично."
  exit 1
else
  echo ""
  echo "  Все проверки пройдены. Контур в рабочем состоянии."
  exit 0
fi
