#!/usr/bin/env bash
# End-to-end smoke checks from Igor's laptop for ActivityWatch-Russian.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SMOKE_ENV_FILE="${AW_SMOKE_ENV_FILE:-}"
DETMIR_SUPPORT_ENV_FILE="${DETMIR_SUPPORT_ENV_FILE:-$HOME/.config/awatch-rus/detmir-support.env}"
for env_candidate in "$REPO_ROOT/private-config/runtime.env" "$HOME/.config/aw-contour-smoke.env" "$SMOKE_ENV_FILE"; do
  if [ -n "$env_candidate" ] && [ -f "$env_candidate" ]; then
    # Load local credentials and site-specific overrides without committing them.
    # Later files override earlier defaults.
    set -a
    . "$env_candidate"
    set +a
  fi
done
if [ -f "$DETMIR_SUPPORT_ENV_FILE" ]; then
  set -a
  . "$DETMIR_SUPPORT_ENV_FILE"
  set +a
fi

# Credential fallbacks:
# - allow running smoke on hosts where AW/WinRM passwords are only stored in support env.
if [ -z "${AW_SSH_PASSWORD:-}" ] && [ -n "${DETMIR_SUPPORT_AW_SSH_PASSWORD:-}" ]; then
  AW_SSH_PASSWORD="$DETMIR_SUPPORT_AW_SSH_PASSWORD"
fi
if [ -z "${AW_SSH_PASSWORD:-}" ] && [ -n "${DETMIR_SUPPORT_SSH_PASSWORD:-}" ]; then
  AW_SSH_PASSWORD="$DETMIR_SUPPORT_SSH_PASSWORD"
fi
if [ -z "${AW_WINRM_PASSWORD:-}" ] && [ -n "${DETMIR_SUPPORT_AW_WINRM_PASSWORD:-}" ]; then
  AW_WINRM_PASSWORD="$DETMIR_SUPPORT_AW_WINRM_PASSWORD"
fi
if [ -z "${AW_WINRM_PASSWORD:-}" ] && [ -n "${DETMIR_SUPPORT_WINRM_PASSWORD:-}" ]; then
  AW_WINRM_PASSWORD="$DETMIR_SUPPORT_WINRM_PASSWORD"
fi
if [ -z "${AW_WINRM_USER:-}" ] && [ -n "${DETMIR_SUPPORT_AW_WINRM_USER:-}" ]; then
  AW_WINRM_USER="$DETMIR_SUPPORT_AW_WINRM_USER"
fi
if [ -z "${AW_WINRM_USER:-}" ] && [ -n "${DETMIR_SUPPORT_WINRM_USER:-}" ]; then
  AW_WINRM_USER="$DETMIR_SUPPORT_WINRM_USER"
fi
WINRM_ANSIBLE_OPTS=()
if [ -n "${AW_WINRM_USER:-}" ]; then
  WINRM_ANSIBLE_OPTS+=( -u "$AW_WINRM_USER" )
fi
if [ -n "${AW_WINRM_PASSWORD:-}" ]; then
  WINRM_ANSIBLE_OPTS+=( -e "ansible_password=$AW_WINRM_PASSWORD" )
fi
export AW_WINRM_USER AW_WINRM_PASSWORD
export AW_SSH_PASSWORD AW_WINRM_PASSWORD

normalize_http_base() {
  local value="${1:-}"
  value="${value%/}"
  case "$value" in
    "") return 1 ;;
    http://*|https://*) printf '%s' "$value" ;;
    *) printf 'http://%s' "$value" ;;
  esac
}

ANSIBLE_DIR="$REPO_ROOT/ansible"
INVENTORY="${AW_SMOKE_INVENTORY:-$ANSIBLE_DIR/inventory.ini}"
REMOTE_SCRIPT_SRC="$REPO_ROOT/scripts/aw-contour-smoke-gateway.sh"
REMOTE_SCRIPT_DST="${AW_SMOKE_REMOTE_SCRIPT:-/usr/local/sbin/aw-contour-smoke.sh}"
DEFAULT_REMOTE_RUST_SRC=""
for rust_candidate in \
  "${CARGO_TARGET_DIR:-}/release/aw-contour-smoke" \
  "$HOME/.cache/detmir-adk-rust-target/release/aw-contour-smoke" \
  "$REPO_ROOT/adk-rust/target/release/aw-contour-smoke"
do
  if [ -n "$rust_candidate" ] && [ -x "$rust_candidate" ]; then
    DEFAULT_REMOTE_RUST_SRC="$rust_candidate"
    break
  fi
done
REMOTE_RUST_SRC="${AW_SMOKE_REMOTE_RUST_SRC:-$DEFAULT_REMOTE_RUST_SRC}"
REMOTE_RUST_DST="${AW_SMOKE_REMOTE_RUST_BIN:-/usr/local/sbin/aw-contour-smoke}"
AW_SERVER="$(normalize_http_base "${AW_SMOKE_AW_SERVER:-http://10.10.10.13:5600}")"
WORKTIME_API="$(normalize_http_base "${AW_SMOKE_WORKTIME_API:-http://10.10.10.13:5610}")"
GRAFANA_URL="$(normalize_http_base "${AW_SMOKE_GRAFANA_URL:-http://10.10.10.11:3000}")"
GRAFANA_USER="${GRAFANA_USER:-igor}"
GRAFANA_PASSWORD="${GRAFANA_PASSWORD:-}"
PROXMOX_HOST="${AW_SMOKE_PROXMOX_HOST:-10.10.10.2}"
AW_HOST="${AW_SMOKE_AW_HOST:-10.10.10.13}"
GRAFANA_HOST="${AW_SMOKE_GRAFANA_HOST:-10.10.10.11}"
WINDOWS_HOST="${AW_SMOKE_WINDOWS_HOST:-${AW_WINDOWS_HOST:-192.168.100.19}}"
AW_SOURCE_HOSTNAME="${AW_SMOKE_SOURCE_HOSTNAME:-${AW_LOGICAL_HOST_ID:-${AW_MONITORED_WINDOWS_HOSTNAME:-SHARKON2025}}}"
LOG_DIR="${AW_SMOKE_LOG_DIR:-$REPO_ROOT/output/smoke}"
RUN_REMOTE="${AW_SMOKE_RUN_REMOTE:-1}"
RUN_WINRM="${AW_SMOKE_RUN_WINRM:-1}"
RUN_SERVER_SYSTEMD="${AW_SMOKE_RUN_SERVER_SYSTEMD:-1}"

NO_PROXY_REQUIRED="localhost,127.0.0.1,$PROXMOX_HOST,$AW_HOST,$GRAFANA_HOST,$WINDOWS_HOST,10.10.10.0/24,192.168.100.0/24"
if [ -n "${no_proxy:-}" ]; then
  export no_proxy="$no_proxy,$NO_PROXY_REQUIRED"
else
  export no_proxy="$NO_PROXY_REQUIRED"
fi
if [ -n "${NO_PROXY:-}" ]; then
  export NO_PROXY="$NO_PROXY,$NO_PROXY_REQUIRED"
else
  export NO_PROXY="$no_proxy"
fi

OK_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
HOST_INACTIVE=0

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

usage() {
  cat <<USAGE
Usage: $(basename "$0") [--skip-remote] [--skip-winrm] [--skip-server-systemd]

Environment overrides:
  AW_SMOKE_AW_SERVER=http://10.10.10.13:5600
  AW_SMOKE_WORKTIME_API=http://10.10.10.13:5610
  AW_SMOKE_GRAFANA_URL=http://10.10.10.11:3000
  AW_WINRM_USER=Администртор
  AW_WINRM_PASSWORD=...
  AW_SMOKE_SOURCE_HOSTNAME=SHARKON2025
  AW_SMOKE_ENV_FILE=$HOME/.config/aw-contour-smoke.env
  AW_SMOKE_LOG_DIR=$REPO_ROOT/output/smoke
  GRAFANA_USER/GRAFANA_PASSWORD via env or a local env file
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-remote) RUN_REMOTE=0 ;;
    --skip-winrm) RUN_WINRM=0 ;;
    --skip-server-systemd) RUN_SERVER_SYSTEMD=0 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 64 ;;
  esac
  shift
done

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

check_local_command() {
  local cmd="$1"
  if have "$cmd"; then
    pass "command available: $cmd"
  else
    fail "command missing: $cmd"
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

check_ssh_banner() {
  local name="$1"
  local host="$2"
  local port="${3:-22}"
  local banner
  banner="$(
    timeout 5 bash -c "exec 3<>/dev/tcp/${host}/${port}; IFS= read -r line <&3; printf '%s' \"\$line\"" 2>/dev/null || true
  )"
  if printf "%s" "$banner" | grep -Eq '^SSH-[0-9]+\.[0-9]+'; then
    pass "$name SSH banner ${host}:${port} ${banner}"
  else
    fail "$name SSH banner ${host}:${port} unavailable"
  fi
}

check_http_code() {
  local name="$1"
  local url="$2"
  local expected="${3:-^2[0-9][0-9]$}"
  local tmp code
  tmp="$(mktemp)"
  code="$(curl -k -sS --connect-timeout 5 --max-time 20 -o "$tmp" -w '%{http_code}' "$url" 2>"$tmp.err")"
  if printf "%s" "$code" | grep -Eq "$expected"; then
    pass "$name HTTP $code $url"
  else
    fail "$name HTTP $code $url"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -40
  fi
  rm -f "$tmp" "$tmp.err"
}

check_http_json_key() {
  local name="$1"
  local url="$2"
  local jq_filter="$3"
  local tmp
  tmp="$(mktemp)"
  if curl -k -fsS --connect-timeout 5 --max-time 20 "$url" -o "$tmp" 2>"$tmp.err" && jq -e "$jq_filter" "$tmp" >/dev/null 2>&1; then
    pass "$name JSON $jq_filter"
    jq -r "$jq_filter" "$tmp" 2>/dev/null | sed 's/^/       /' | head -5
  else
    fail "$name JSON $jq_filter"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -40
  fi
  rm -f "$tmp" "$tmp.err"
}

check_http_json_key_basic_auth() {
  local name="$1"
  local url="$2"
  local jq_filter="$3"
  local tmp
  tmp="$(mktemp)"
  if curl -k -fsS -u "$GRAFANA_USER:$GRAFANA_PASSWORD" --connect-timeout 5 --max-time 20 "$url" -o "$tmp" 2>"$tmp.err" && jq -e "$jq_filter" "$tmp" >/dev/null 2>&1; then
    pass "$name JSON $jq_filter"
    jq -r "$jq_filter" "$tmp" 2>/dev/null | sed 's/^/       /' | head -5
  else
    fail "$name JSON $jq_filter"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -40
  fi
  rm -f "$tmp" "$tmp.err"
}

check_http_code_basic_auth() {
  local name="$1"
  local url="$2"
  local expected="${3:-^2[0-9][0-9]$}"
  local tmp code
  tmp="$(mktemp)"
  code="$(curl -k -sS -u "$GRAFANA_USER:$GRAFANA_PASSWORD" --connect-timeout 5 --max-time 20 -o "$tmp" -w '%{http_code}' "$url" 2>"$tmp.err")"
  if printf "%s" "$code" | grep -Eq "$expected"; then
    pass "$name HTTP $code $url"
  else
    fail "$name HTTP $code $url"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -40
  fi
  rm -f "$tmp" "$tmp.err"
}

check_grafana_influx_health() {
  local name="Grafana InfluxDB-AW datasource health"
  local tmp
  tmp="$(mktemp)"
  if curl -k -fsS -u "$GRAFANA_USER:$GRAFANA_PASSWORD" --connect-timeout 5 --max-time 20 "$GRAFANA_URL/api/datasources/uid/influxdb_aw/health" -o "$tmp" 2>"$tmp.err" && jq -e '.status == "OK"' "$tmp" >/dev/null 2>&1; then
    pass "$name"
    jq -r '.message // .status // .' "$tmp" 2>/dev/null | sed 's/^/       /' | head -5
  else
    fail "$name"
    sed 's/^/       /' "$tmp.err" "$tmp" 2>/dev/null | head -40
  fi
  rm -f "$tmp" "$tmp.err"
}

check_grafana_aw_main_dashboard_queries() {
  local name="Grafana detmir-aw-main worktime panel queries"
  local tmp
  tmp="$(mktemp)"
  if curl -k -fsS -u "$GRAFANA_USER:$GRAFANA_PASSWORD" --connect-timeout 5 --max-time 20 "$GRAFANA_URL/api/dashboards/uid/detmir-aw-main" -o "$tmp" 2>"$tmp.err" && \
    jq -e '
      def query: (.targets[0].query // "");
      def has_panel($title; $checks):
        any(.dashboard.panels[]?; .title == $title and (query as $q | all($checks[]; $q | contains(.))));
      has_panel("Активность RDP по часам"; [
        "aw_rdp_worktime_hourly",
        "r.user_id !~ /\\$$/",
        "r.user_id !~ /�/",
        "group(columns: [\"_time\", \"user\"])",
        "max(column: \"_value\")"
      ]) and
      has_panel("Сегодня: активность по сотрудникам"; [
        "aw_rdp_worktime_daily",
        "group(columns: [\"report_date\", \"user\"])",
        "max(column: \"_value\")",
        "last()"
      ]) and
      has_panel("Все сотрудники: активное время по дням"; [
        "aw_rdp_worktime_summary_daily",
        "Все сотрудники, ч"
      ])
    ' "$tmp" >/dev/null 2>&1; then
    pass "$name"
  else
    fail "$name missing expected DetMir worktime/dedupe query contract"
    jq -r '.dashboard.panels[]? | select(.title == "Активность RDP по часам" or .title == "Сегодня: активность по сотрудникам" or .title == "Все сотрудники: активное время по дням") | "\(.title): " + ((.targets[0].query // "") | gsub("\n"; " "))' "$tmp" 2>/dev/null | sed 's/^/       /' | head -20
    sed 's/^/       /' "$tmp.err" 2>/dev/null | head -20
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

ansible_shell() {
  local group="$1"
  local command="$2"
  ANSIBLE_NOCOLOR=1 ansible "$group" -i "$INVENTORY" -m shell -a "$command"
}

ansible_win_shell() {
  local group="$1"
  local command="$2"
  ANSIBLE_NOCOLOR=1 ansible "$group" -i "$INVENTORY" -m win_shell -a "$command"
}

check_ansible_shell() {
  local name="$1"
  local group="$2"
  local command="$3"
  local tmp
  tmp="$(mktemp)"
  if ansible_shell "$group" "$command" >"$tmp" 2>&1; then
    pass "$name"
    sed 's/^/       /' "$tmp" | head -80
  else
    fail "$name"
    sed 's/^/       /' "$tmp" | head -120
  fi
  rm -f "$tmp"
}

check_ansible_win_shell() {
  local name="$1"
  local group="$2"
  local command="$3"
  local tmp
  tmp="$(mktemp)"
  if ANSIBLE_NOCOLOR=1 ansible "$group" -i "$INVENTORY" -m win_shell -a "$command" "${WINRM_ANSIBLE_OPTS[@]}" >"$tmp" 2>&1; then
    pass "$name"
    sed 's/^/       /' "$tmp" | head -80
  else
    fail "$name"
    sed 's/^/       /' "$tmp" | head -120
  fi
  rm -f "$tmp"
}

check_ansible_win_shell_warn() {
  local name="$1"
  local group="$2"
  local command="$3"
  local tmp
  tmp="$(mktemp)"
  if ANSIBLE_NOCOLOR=1 ansible "$group" -i "$INVENTORY" -m win_shell -a "$command" "${WINRM_ANSIBLE_OPTS[@]}" >"$tmp" 2>&1; then
    pass "$name"
    sed 's/^/       /' "$tmp" | head -80
  else
    warn "$name returned non-zero"
    sed 's/^/       /' "$tmp" | head -120
  fi
  rm -f "$tmp"
}

check_ansible_module() {
  local name="$1"
  local group="$2"
  local module="$3"
  local args="${4:-}"
  local tmp
  tmp="$(mktemp)"
  if ANSIBLE_NOCOLOR=1 ansible "$group" -i "$INVENTORY" -m "$module" ${args:+-a "$args"} "${WINRM_ANSIBLE_OPTS[@]}" >"$tmp" 2>&1; then
    pass "$name"
    sed 's/^/       /' "$tmp" | head -80
  else
    fail "$name"
    sed 's/^/       /' "$tmp" | head -120
  fi
  rm -f "$tmp"
}

read_activitywatch_context() {
  local bucket_id tmp last_ts active event_epoch now age_sec
  bucket_id="aw-worktime-sessions_${AW_SOURCE_HOSTNAME}"
  tmp="$(mktemp)"
  if curl -fsS --connect-timeout 5 --max-time 20 "$AW_SERVER/api/0/buckets/$bucket_id/events?limit=1" -o "$tmp" 2>"$tmp.err"; then
    last_ts="$(jq -r '.[0].timestamp // empty' "$tmp" 2>/dev/null)"
    active="$(jq -r '.[0].data.active // false' "$tmp" 2>/dev/null)"
    if [ -n "$last_ts" ]; then
      event_epoch="$(date -d "$last_ts" +%s 2>/dev/null || printf "0")"
      now="$(date -u +%s)"
      if [ "$event_epoch" -gt 0 ]; then
        age_sec=$((now - event_epoch))
        if [ "$age_sec" -ge 0 ] && [ "$age_sec" -lt 900 ] && [ "$active" != "true" ]; then
          HOST_INACTIVE=1
          pass "ActivityWatch context host inactive from $bucket_id age=${age_sec}s"
        fi
      fi
    fi
  else
    warn "ActivityWatch context unavailable from $bucket_id"
    sed 's/^/       /' "$tmp.err" | head -20
  fi
  rm -f "$tmp" "$tmp.err"
}

classify_bucket_age() {
  local bucket="$1"
  local age_sec="$2"
  if [ "$bucket" = "aw-watcher-window" ] && [ "$HOST_INACTIVE" = "1" ]; then
    printf "inactive"
    return
  fi
  case "$bucket" in
    aw-dlp-incidents|aw-dlp-review|aw-dlp-rules|aw-session-events)
      if [ "$age_sec" -lt 86400 ]; then
        printf "fresh"
      else
        printf "event-driven"
      fi
      ;;
    *)
      if [ "$age_sec" -lt 3600 ]; then
        printf "fresh"
      elif [ "$age_sec" -lt 86400 ]; then
        printf "stale"
      else
        printf "dead"
      fi
      ;;
  esac
}

check_bucket_freshness() {
  local bucket="$1"
  local bucket_id="${bucket}_${AW_SOURCE_HOSTNAME}"
  local tmp last_ts last_id event_epoch now age_sec status meta_ts
  tmp="$(mktemp)"
  if ! curl -fsS --connect-timeout 5 --max-time 20 "$AW_SERVER/api/0/buckets/$bucket_id/events?limit=1" -o "$tmp" 2>"$tmp.err"; then
    fail "bucket $bucket_id query failed"
    sed 's/^/       /' "$tmp.err" | head -20
    rm -f "$tmp" "$tmp.err"
    return
  fi

  last_ts="$(jq -r '.[0].timestamp // empty' "$tmp" 2>/dev/null)"
  last_id="$(jq -r '.[0].id // 0' "$tmp" 2>/dev/null)"
  rm -f "$tmp" "$tmp.err"
  if [ "$bucket" = "aw-watcher-afk" ]; then
    meta_ts="$(curl -fsS --connect-timeout 5 --max-time 20 "$AW_SERVER/api/0/buckets/$bucket_id" 2>/dev/null | jq -r '.metadata.end // empty' 2>/dev/null || true)"
    if [ -n "$meta_ts" ]; then
      last_ts="$meta_ts"
    fi
  fi

  if [ -z "$last_ts" ]; then
    case "$bucket" in
      aw-watcher-window)
        if [ "$HOST_INACTIVE" = "1" ]; then
          pass "bucket $bucket_id inactive/empty while host inactive"
        else
          fail "bucket $bucket_id empty"
        fi
        ;;
      aw-dlp-incidents|aw-dlp-review|aw-dlp-rules|aw-session-events)
        warn "bucket $bucket_id empty/event-driven"
        ;;
      *)
        fail "bucket $bucket_id empty"
        ;;
    esac
    return
  fi

  event_epoch="$(date -d "$last_ts" +%s 2>/dev/null || printf "0")"
  now="$(date -u +%s)"
  if [ "$event_epoch" -le 0 ]; then
    warn "bucket $bucket_id has unparsable timestamp: $last_ts"
    return
  fi
  age_sec=$((now - event_epoch))
  status="$(classify_bucket_age "$bucket" "$age_sec")"
  case "$status" in
    fresh|event-driven|inactive)
      pass "bucket $bucket_id $status age=${age_sec}s id=$last_id"
      ;;
    stale)
      warn "bucket $bucket_id stale age=${age_sec}s id=$last_id"
      ;;
    *)
      fail "bucket $bucket_id dead age=${age_sec}s id=$last_id"
      ;;
  esac
}

run_remote_proxmox_script() {
  if [ "$RUN_REMOTE" != "1" ]; then
    skip "remote 10.10.10.2 smoke skipped"
    return
  fi
  if ! have ansible; then
    fail "ansible unavailable; cannot deploy/run remote script"
    return
  fi
  if [ ! -f "$REMOTE_SCRIPT_SRC" ]; then
    fail "remote script source missing: $REMOTE_SCRIPT_SRC"
    return
  fi

  section "Deploy Remote Script To 10.10.10.2"
  if [ -x "$REMOTE_RUST_SRC" ]; then
    if ANSIBLE_NOCOLOR=1 ansible proxmox -i "$INVENTORY" -m copy -a "src=$REMOTE_RUST_SRC dest=$REMOTE_RUST_DST owner=root group=root mode=0755" >/tmp/aw-smoke-copy-rust.$$ 2>&1; then
      pass "remote Rust smoke deployed to $REMOTE_RUST_DST"
    else
      warn "remote Rust smoke deploy failed; shell fallback remains available"
      sed 's/^/       /' /tmp/aw-smoke-copy-rust.$$ | head -120
    fi
    rm -f /tmp/aw-smoke-copy-rust.$$
  else
    skip "remote Rust smoke binary not found: $REMOTE_RUST_SRC"
  fi

  if ANSIBLE_NOCOLOR=1 ansible proxmox -i "$INVENTORY" -m copy -a "src=$REMOTE_SCRIPT_SRC dest=$REMOTE_SCRIPT_DST owner=root group=root mode=0755" >/tmp/aw-smoke-copy.$$ 2>&1; then
    pass "remote script deployed to $REMOTE_SCRIPT_DST"
  else
    fail "remote script deploy failed"
    sed 's/^/       /' /tmp/aw-smoke-copy.$$ | head -120
    rm -f /tmp/aw-smoke-copy.$$
    return
  fi
  rm -f /tmp/aw-smoke-copy.$$

  section "Remote 10.10.10.2 Smoke"
  local tmp
  tmp="$(mktemp)"
  if ANSIBLE_NOCOLOR=1 ansible proxmox -i "$INVENTORY" -m shell -a "$REMOTE_SCRIPT_DST" >"$tmp" 2>&1; then
    pass "remote 10.10.10.2 smoke completed"
    sed 's/^/       /' "$tmp"
  else
    fail "remote 10.10.10.2 smoke failed"
    sed 's/^/       /' "$tmp"
  fi
  rm -f "$tmp"
}

mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/aw-contour-smoke-$(date +%Y%m%d-%H%M%S).log"
exec > >(tee "$LOG_FILE") 2>&1

section "Run Context"
printf "repo=%s\ninventory=%s\nlog=%s\n" "$REPO_ROOT" "$INVENTORY" "$LOG_FILE" | sed 's/^/       /'
date -Is | sed 's/^/       /'

section "Local Prerequisites"
for cmd in bash curl jq timeout ansible ssh; do
  check_local_command "$cmd"
done
printf "       no_proxy=%s\n" "$no_proxy"

section "Laptop Network"
ip -br addr 2>/dev/null | sed 's/^/       /' || true
ip route get "$PROXMOX_HOST" 2>/dev/null | sed 's/^/       /' || warn "no route detail for $PROXMOX_HOST"
ip route get "$AW_HOST" 2>/dev/null | sed 's/^/       /' || warn "no route detail for $AW_HOST"

section "TCP Surface"
check_tcp "Proxmox SSH" "$PROXMOX_HOST" 22
check_tcp "Proxmox HTTP" "$PROXMOX_HOST" 80
check_tcp "Proxmox HTTPS" "$PROXMOX_HOST" 443
check_tcp "Proxmox GUI" "$PROXMOX_HOST" 8006
check_tcp "1C company API" "$PROXMOX_HOST" 8710
check_tcp "AW server" "$AW_HOST" 5600
check_tcp "AW worktime API" "$AW_HOST" 5610
check_tcp "Grafana" "$GRAFANA_HOST" 3000
check_tcp "Windows WinRM" "$WINDOWS_HOST" 5985
check_tcp "Windows SSH" "$WINDOWS_HOST" 22
check_ssh_banner "Windows SSH" "$WINDOWS_HOST" 22

section "ActivityWatch HTTP"
check_http_json_key "AW server info" "$AW_SERVER/api/0/info" '.version'
check_http_code "AW settings CORS" "$AW_SERVER/api/0/settings/" '^200$'
check_http_code "AW WebUI" "$AW_SERVER/" '^200$'
check_http_json_key "AW buckets list" "$AW_SERVER/api/0/buckets/" 'keys | length'

section "ActivityWatch Buckets"
read_activitywatch_context
for bucket in \
  aw-watcher-afk \
  aw-watcher-window \
  aw-worktime-sessions \
  aw-session-events \
  aw-dlp-endpoint-signals \
  aw-dlp-incidents \
  aw-dlp-review \
  aw-dlp-rules
do
  check_bucket_freshness "$bucket"
done

section "Worktime API"
check_http_json_key "worktime health" "$WORKTIME_API/health" '.status // .ok // .'
check_http_code "worktime today html" "$WORKTIME_API/reports/worktime/today?host=$AW_SOURCE_HOSTNAME&day=today&format=html" '^200$'
check_http_json_key "worktime today json" "$WORKTIME_API/reports/worktime/today?host=$AW_SOURCE_HOSTNAME&day=today" '.host // .report.host // .[0].host // .'
check_http_code "worktime management html" "$WORKTIME_API/reports/worktime/management?host=$AW_SOURCE_HOSTNAME&day=today&format=html" '^200$'

section "Gateway And 1C HTTP"
check_http_code "gateway healthz" "https://$PROXMOX_HOST/healthz" '^200$'
check_http_code "gateway proxmox redirect" "https://$PROXMOX_HOST/go/proxmox-gui" '^302$'
check_http_code "gateway file1c brief redirect" "https://$PROXMOX_HOST/go/file1c-brief" '^302$'
check_http_code "gateway file1c actions redirect" "https://$PROXMOX_HOST/go/file1c-actions" '^302$'
check_http_code "1C /health" "http://$PROXMOX_HOST:8710/health" '^200$'
check_http_code "1C /api/health" "http://$PROXMOX_HOST:8710/api/health" '^200$'
check_http_code "1C manager brief" "http://$PROXMOX_HOST:8710/manager/brief" '^200$'
check_http_code "1C manager actions" "http://$PROXMOX_HOST:8710/manager/actions" '^200$'
check_http_code "1C manager recovery" "http://$PROXMOX_HOST:8710/manager/recovery" '^200$'
check_http_code "1C weekly digest" "http://$PROXMOX_HOST:8710/manager/digest/weekly" '^200$'

section "Grafana HTTP"
check_http_json_key "Grafana health" "$GRAFANA_URL/api/health" '.database // .version // .commit'
check_http_code "Grafana dashboards page" "$GRAFANA_URL/dashboards" '^200$|^302$'

if [ -n "${GRAFANA_USER:-}" ] && [ -n "${GRAFANA_PASSWORD:-}" ]; then
  section "Grafana Authenticated API"
  check_http_json_key_basic_auth "Grafana datasources" "$GRAFANA_URL/api/datasources" 'length'
  check_grafana_influx_health
  check_grafana_aw_main_dashboard_queries
else
  skip "Grafana authenticated datasource checks need GRAFANA_USER and GRAFANA_PASSWORD env"
fi

if [ "$RUN_SERVER_SYSTEMD" = "1" ] && have ansible; then
  if [ -z "${AW_SSH_PASSWORD:-}" ] && [ -z "${AW_SUDO_PASSWORD:-}" ] && [ -z "${AW_SSH_KEY_PATH:-}" ]; then
    fail "AW server systemd checks blocked: missing AW_SSH_PASSWORD or AW_SUDO_PASSWORD (set AW_SSH_PASSWORD / AW_SUDO_PASSWORD or AW_SSH_KEY_PATH)."
  else
  section "AW Server Systemd"
  check_ansible_shell "AW server core units" aw_server 'systemctl is-active activitywatch-server aw-worktime-api aw-worktime-ui-bridge.timer aw-rus-healthd.timer aw-worktime-influx-exporter.timer aw-dlp-influx-exporter.timer aw-worktime-autoheal.timer'
  check_ansible_shell "AW server failed units" aw_server 'failed=$(systemctl --failed --no-legend | awk "{print \$1}" | grep -E "activitywatch|aw-|influx|grafana|prometheus" || true); test -z "$failed" && echo "no AW-related failed units" || { echo "$failed"; exit 1; }'
  check_ansible_shell "AW server local health script" aw_server 'test -x /opt/activitywatch/health-check.sh && /opt/activitywatch/health-check.sh || test -x /usr/local/bin/health-check.sh && /usr/local/bin/health-check.sh || echo "health-check script not installed"'
  fi
else
  skip "AW server systemd checks skipped"
fi

if [ "$RUN_WINRM" = "1" ] && have ansible; then
  if [ -z "${AW_WINRM_PASSWORD:-}" ]; then
    fail "Windows WinRM checks blocked: missing AW_WINRM_PASSWORD (set AW_WINRM_PASSWORD or export DETMIR_SUPPORT_AW_WINRM_PASSWORD/DETMIR_SUPPORT_WINRM_PASSWORD in $DETMIR_SUPPORT_ENV_FILE)."
  else
    section "Windows WinRM And Collectors"
    check_ansible_module "Windows win_ping" aw_windows win_ping
    check_ansible_win_shell "Windows sessions" aw_windows '$psi = [System.Diagnostics.ProcessStartInfo]::new(); $psi.FileName = "$env:SystemRoot\System32\query.exe"; $psi.Arguments = "user"; $psi.UseShellExecute = $false; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true; $psi.StandardOutputEncoding = [System.Text.Encoding]::GetEncoding(866); $psi.StandardErrorEncoding = [System.Text.Encoding]::GetEncoding(866); $p = [System.Diagnostics.Process]::Start($psi); $out = $p.StandardOutput.ReadToEnd(); $err = $p.StandardError.ReadToEnd(); $p.WaitForExit(); [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $out; if ($err) { $err }; if ($out -match "USERNAME|ПОЛЬЗОВАТЕЛЬ|администратор|Администратор") { exit 0 } else { exit $p.ExitCode }'
    check_ansible_win_shell_warn "Windows collector processes" aw_windows '$p = Get-Process aw-watcher-afk,aw-watcher-window -ErrorAction SilentlyContinue; if ($p) { $p | Select-Object Name,Id,SessionId,StartTime | Format-Table -AutoSize } else { "no aw-watcher-afk/window process visible to this WinRM session" }'
    check_ansible_win_shell "Windows ActivityWatch tasks" aw_windows 'schtasks /Query /TN "ActivityWatch Recovery" /FO LIST /V; Get-Content -Raw "C:\ProgramData\AWatch-rus\deployment-config.json" | ConvertFrom-Json | Select-Object -ExpandProperty userTasks | Select-Object LaunchTaskName,UserId | Format-Table -AutoSize'
  fi
else
  skip "Windows WinRM checks skipped"
fi

run_remote_proxmox_script

section "Existing Repo Checks"
if [ -x "$REPO_ROOT/check-aw-data.sh" ]; then
  if "$REPO_ROOT/check-aw-data.sh"; then pass "check-aw-data.sh completed"; else fail "check-aw-data.sh failed"; fi
else
  skip "check-aw-data.sh missing"
fi
if [ -x "$REPO_ROOT/check-aw-full.sh" ]; then
  if AW_SMOKE_AW_SERVER="$AW_SERVER" AW_SMOKE_SOURCE_HOSTNAME="$AW_SOURCE_HOSTNAME" AW_SMOKE_WINDOWS_HOST="$WINDOWS_HOST" "$REPO_ROOT/check-aw-full.sh"; then pass "check-aw-full.sh completed"; else fail "check-aw-full.sh failed"; fi
else
  skip "check-aw-full.sh missing"
fi

section "Summary"
printf "OK=%s WARN=%s FAIL=%s SKIP=%s\n" "$OK_COUNT" "$WARN_COUNT" "$FAIL_COUNT" "$SKIP_COUNT"
printf "Log: %s\n" "$LOG_FILE"

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 2
fi
exit 0
