#!/usr/bin/env bash
# Smoke checks for the Proxmox/gateway/1C host 10.10.10.2.

set -uo pipefail

TARGET_ROOT="${CARGO_TARGET_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." 2>/dev/null && pwd)/adk-rust/target}"
RUST_BIN="${AW_CONTOUR_SMOKE_RUST:-}"
rust_candidates=()
if [ -n "$RUST_BIN" ]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/aw-contour-smoke"
  "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." 2>/dev/null && pwd)/adk-rust/target/release/aw-contour-smoke"
  "/usr/local/sbin/aw-contour-smoke"
  "/usr/local/bin/aw-contour-smoke"
)

for candidate in "${rust_candidates[@]}"; do
  if [ -x "$candidate" ]; then
    if [ "$#" -gt 0 ]; then
      exec "$candidate" "$@"
    fi
    exec "$candidate" --mode proxmox-remote
  fi
done

OK_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

if [ -t 1 ]; then
  RED='\033[0;31m'
  GREEN='\033[0;32m'
  YELLOW='\033[1;33m'
  CYAN='\033[0;36m'
  NC='\033[0m'
else
  RED=''
  GREEN=''
  YELLOW=''
  CYAN=''
  NC=''
fi

pass() { OK_COUNT=$((OK_COUNT + 1)); printf "%b[OK]%b   %s\n" "$GREEN" "$NC" "$*"; }
warn() { WARN_COUNT=$((WARN_COUNT + 1)); printf "%b[WARN]%b %s\n" "$YELLOW" "$NC" "$*"; }
fail() { FAIL_COUNT=$((FAIL_COUNT + 1)); printf "%b[FAIL]%b %s\n" "$RED" "$NC" "$*"; }
skip() { SKIP_COUNT=$((SKIP_COUNT + 1)); printf "%b[SKIP]%b %s\n" "$YELLOW" "$NC" "$*"; }

section() {
  printf "\n%b== %s ==%b\n" "$CYAN" "$*" "$NC"
}

have() {
  command -v "$1" >/dev/null 2>&1
}

check_command() {
  local name="$1"
  shift
  local output
  if output="$("$@" 2>&1)"; then
    pass "$name"
    [ -n "$output" ] && printf "%s\n" "$output" | sed 's/^/       /'
  else
    fail "$name"
    [ -n "$output" ] && printf "%s\n" "$output" | sed 's/^/       /'
  fi
}

check_service() {
  local unit="$1"
  if ! systemctl list-unit-files "$unit" >/dev/null 2>&1; then
    skip "$unit is not installed"
    return
  fi
  if systemctl is-active --quiet "$unit"; then
    pass "$unit active"
  else
    fail "$unit inactive or failed"
    systemctl --no-pager --lines=8 status "$unit" 2>&1 | sed 's/^/       /'
  fi
}

check_timer() {
  local unit="$1"
  if ! systemctl list-unit-files "$unit" >/dev/null 2>&1; then
    skip "$unit is not installed"
    return
  fi
  if systemctl is-active --quiet "$unit"; then
    pass "$unit active"
  else
    fail "$unit inactive or failed"
    systemctl --no-pager --lines=8 status "$unit" 2>&1 | sed 's/^/       /'
  fi
}

check_tcp() {
  local name="$1"
  local host="$2"
  local port="$3"
  if timeout 4 bash -c ":</dev/tcp/${host}/${port}" >/dev/null 2>&1; then
    pass "$name TCP ${host}:${port}"
  else
    fail "$name TCP ${host}:${port}"
  fi
}

check_http_code() {
  local name="$1"
  local url="$2"
  local expected="${3:-^2[0-9][0-9]$}"
  local tmp code
  tmp="$(mktemp)"
  code="$(curl -k -sS --connect-timeout 5 --max-time 15 -o "$tmp" -w '%{http_code}' "$url" 2>"$tmp.err")"
  if printf "%s" "$code" | grep -Eq "$expected"; then
    pass "$name HTTP $code $url"
  else
    fail "$name HTTP $code $url"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -40
  fi
  rm -f "$tmp" "$tmp.err"
}

check_http_redirect() {
  local name="$1"
  local url="$2"
  local expected_code="${3:-^30[1278]$}"
  local expected_location="${4:-}"
  local tmp code location
  tmp="$(mktemp)"
  code="$(curl -k -sS -I --connect-timeout 5 --max-time 15 -o "$tmp" -w '%{http_code}' "$url" 2>"$tmp.err")"
  location="$(awk 'BEGIN{IGNORECASE=1} /^location:/ {sub(/\r$/,""); print $0}' "$tmp" | tail -1)"
  if printf "%s" "$code" | grep -Eq "$expected_code" && { [ -z "$expected_location" ] || printf "%s" "$location" | grep -Fq "$expected_location"; }; then
    pass "$name HTTP $code ${location:-$url}"
  else
    fail "$name HTTP $code ${location:-$url}"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -40
  fi
  rm -f "$tmp" "$tmp.err"
}

check_docker_container() {
  local name="$1"
  if ! have docker; then
    skip "docker command unavailable"
    return
  fi
  if docker ps --format '{{.Names}}' 2>/dev/null | grep -Fxq "$name"; then
    pass "docker container $name running"
    docker ps --filter "name=^/${name}$" --format '       {{.Names}} {{.Status}} {{.Ports}}'
  else
    fail "docker container $name not running"
    docker ps -a --filter "name=^/${name}$" --format '       {{.Names}} {{.Status}} {{.Ports}}' 2>/dev/null || true
  fi
}

section "Host"
hostnamectl 2>/dev/null | sed 's/^/       /' || hostname | sed 's/^/       /'
date -Is | sed 's/^/       /'
uptime | sed 's/^/       /'

section "Core Services"
for unit in \
  nginx.service \
  pveproxy.service \
  pvedaemon.service \
  pvestatd.service \
  pve-cluster.service \
  docker.service \
  aw-1c-company-api.service \
  aw-pve-webadmin-logger.service
do
  check_service "$unit"
done

section "Timers"
for unit in \
  aw-1c-ingest.timer \
  aw-1c-proofcheck.timer \
  aw-1c-manager-brief.timer \
  aw-1c-recovery-brief.timer \
  aw-1c-weekly-digest.timer
do
  check_timer "$unit"
done
systemctl list-timers --all --no-pager 2>/dev/null | grep -E 'aw-1c|NEXT|LEFT|PASSED' | sed 's/^/       /' || true

section "Ports"
check_tcp "nginx http" 127.0.0.1 80
check_tcp "nginx https" 127.0.0.1 443
check_tcp "proxmox web" 127.0.0.1 8006
check_tcp "1C company API" 10.10.10.2 8710
check_tcp "clickhouse native" 127.0.0.1 9000
check_tcp "clickhouse http" 127.0.0.1 8123
ss -tulpn | grep -E ':(80|443|8006|8710|8123|9000)\b' | sed 's/^/       /' || true

section "Gateway HTTP"
check_http_code "nginx healthz" "https://127.0.0.1/healthz" '^200$'
check_http_code "go proxmox gui protected" "https://127.0.0.1/go/proxmox-gui" '^401$'
check_http_code "go file1c brief protected" "https://127.0.0.1/go/file1c-brief" '^401$'
check_http_code "go file1c actions protected" "https://127.0.0.1/go/file1c-actions" '^401$'

section "1C Company API"
check_http_code "1C root redirect" "http://10.10.10.2:8710/" '^307$'
check_http_code "1C /health" "http://10.10.10.2:8710/health" '^200$'
check_http_code "1C /api/health" "http://10.10.10.2:8710/api/health" '^200$'
check_http_code "1C manager brief" "http://10.10.10.2:8710/manager/brief" '^200$'
check_http_code "1C manager actions" "http://10.10.10.2:8710/manager/actions" '^200$'
check_http_code "1C manager recovery" "http://10.10.10.2:8710/manager/recovery" '^200$'
check_http_code "1C weekly digest" "http://10.10.10.2:8710/manager/digest/weekly" '^200$'

section "ClickHouse"
check_docker_container "aw-rus-1c-clickhouse"
check_http_code "ClickHouse ping" "http://127.0.0.1:8123/ping" '^200$'
if have docker && docker ps --format '{{.Names}}' | grep -Fxq aw-rus-1c-clickhouse; then
  check_command "ClickHouse SELECT 1" docker exec aw-rus-1c-clickhouse clickhouse-client --query "SELECT 1"
fi

section "System Capacity"
df -h / /var /opt 2>/dev/null | sed 's/^/       /'
free -h 2>/dev/null | sed 's/^/       /' || true

section "Summary"
printf "OK=%s WARN=%s FAIL=%s SKIP=%s\n" "$OK_COUNT" "$WARN_COUNT" "$FAIL_COUNT" "$SKIP_COUNT"

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 2
fi
exit 0
