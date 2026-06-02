#!/usr/bin/env bash
set -euo pipefail

WORKTIME_BASE_URL="${WORKTIME_BASE_URL:-http://127.0.0.1:5610}"
WORKTIME_PREWARM_TIMEOUT_SECONDS="${WORKTIME_PREWARM_TIMEOUT_SECONDS:-45}"
WORKTIME_PREWARM_HEALTH_TIMEOUT_SECONDS="${WORKTIME_PREWARM_HEALTH_TIMEOUT_SECONDS:-10}"
WORKTIME_PREWARM_READY_TIMEOUT_SECONDS="${WORKTIME_PREWARM_READY_TIMEOUT_SECONDS:-60}"
WORKTIME_PREWARM_READY_INTERVAL_SECONDS="${WORKTIME_PREWARM_READY_INTERVAL_SECONDS:-2}"
WORKTIME_PREWARM_HOST="${WORKTIME_PREWARM_HOST:-${AW_WORKTIME_HOST:-SHARKON2025}}"
WORKTIME_PREWARM_PROFILE="${WORKTIME_PREWARM_PROFILE:-full}"

log() {
  printf '%s %s\n' "$(date '+%F %T')" "$*"
}

probe() {
  local url="$1"
  local timeout="$2"
  local tmp
  tmp="$(mktemp)"
  local code
  code="$(curl -sS --max-time "$timeout" -o /dev/null -D "$tmp" -w '%{http_code}' "$url" 2>/dev/null || true)"
  if [[ "$code" =~ ^2 ]]; then
    local cache
    cache="$(tr '\r' '\n' < "$tmp" | awk 'tolower($0) ~ /^x-aw-worktime-cache:/ {print $2; exit}' || true)"
    local reason
    reason="$(tr '\r' '\n' < "$tmp" | awk 'tolower($0) ~ /^x-aw-worktime-cache-reason:/ {print $2; exit}' || true)"
    rm -f "$tmp"
    log "ok code=$code cache=${cache:-none} reason=${reason:-none} url=$url"
    return 0
  fi
  rm -f "$tmp"
  log "warn code=${code:-000} url=$url"
  return 1
}

wait_until_ready() {
  local deadline
  deadline=$(( $(date +%s) + WORKTIME_PREWARM_READY_TIMEOUT_SECONDS ))
  while true; do
    if probe "$WORKTIME_BASE_URL/health" "$WORKTIME_PREWARM_HEALTH_TIMEOUT_SECONDS"; then
      return 0
    fi
    if [[ "$(date +%s)" -ge "$deadline" ]]; then
      return 1
    fi
    sleep "$WORKTIME_PREWARM_READY_INTERVAL_SECONDS"
  done
}

if ! wait_until_ready; then
  log "health readiness timed out; skip prewarm"
  exit 0
fi

full_urls=(
  "$WORKTIME_BASE_URL/reports/worktime/today?day=today&format=csv&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/today?day=today&format=json&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/today?day=today&format=html&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/management?day=today&format=csv&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/management?day=today&format=json&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/management?day=today&format=html&host=$WORKTIME_PREWARM_HOST"
)

startup_urls=(
  "$WORKTIME_BASE_URL/reports/worktime/today?day=today&format=csv&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/today?day=today&format=json&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/management?day=today&format=json&host=$WORKTIME_PREWARM_HOST"
  "$WORKTIME_BASE_URL/reports/worktime/management?day=today&format=csv&host=$WORKTIME_PREWARM_HOST"
)

case "$WORKTIME_PREWARM_PROFILE" in
  full)
    urls=("${full_urls[@]}")
    ;;
  startup)
    urls=("${startup_urls[@]}")
    ;;
  *)
    log "unknown profile=$WORKTIME_PREWARM_PROFILE"
    exit 0
    ;;
esac

failures=0
for url in "${urls[@]}"; do
  if ! probe "$url" "$WORKTIME_PREWARM_TIMEOUT_SECONDS"; then
    failures=$((failures + 1))
  fi
done

if [[ "$failures" -gt 0 ]]; then
  log "completed profile=$WORKTIME_PREWARM_PROFILE with failures=$failures"
else
  log "completed profile=$WORKTIME_PREWARM_PROFILE successfully"
fi

exit 0
