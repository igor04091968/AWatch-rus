#!/bin/bash
# check-aw-full.sh - Полная проверка ActivityWatch: сервер + RDP-хост
# Сервер: http://10.10.10.13:5600
# RDP-хост: 192.168.100.19 (logical host id SHARKON2025)

if [[ "${CHECK_AW_FULL_FORCE_LEGACY:-0}" != "1" ]]; then
  ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  for candidate in \
    "${CHECK_AW_FULL_RUST:-}" \
    "${CARGO_TARGET_DIR:-}/release/check-aw-full" \
    "$ROOT_DIR/adk-rust/target/release/check-aw-full" \
    "/usr/local/bin/check-aw-full"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
      exec "$candidate" "$@"
    fi
  done
fi

SERVER="${CHECK_AW_FULL_SERVER:-${AW_SMOKE_AW_SERVER:-${AW_SERVER:-http://10.10.10.13:5600}}}"
HOSTNAME_FILTER="${CHECK_AW_FULL_HOST:-${AW_SMOKE_SOURCE_HOSTNAME:-${AW_LOGICAL_HOST_ID:-${AW_MONITORED_WINDOWS_HOSTNAME:-SHARKON2025}}}}"
RDP_HOST="${CHECK_AW_FULL_RDP_HOST:-${AW_SMOKE_WINDOWS_HOST:-${AW_WINDOWS_HOST:-192.168.100.19}}}"
NOW=$(date -u +%s)
HOST_INACTIVE=false
GUARD_HEALTHY=false
DLP_ENABLED="${AW_DLP_ENABLED:-${DETMIR_DLP_ENABLED:-true}}"
case "${DLP_ENABLED,,}" in
  0|false|no|off) DLP_ENABLED=false ;;
  *) DLP_ENABLED=true ;;
esac

classify_bucket_age() {
    local bucket="$1"
    local age_sec="$2"

    case "$bucket" in
        aw-watcher-window)
            if [ "$HOST_INACTIVE" = "true" ]; then
                printf 'INACTIVE|%s' "${CYAN}INACTIVE${NC}"
                return
            fi
            ;;
        aw-dlp-endpoint-signals)
            if [ "$HOST_INACTIVE" = "true" ] && [ "$GUARD_HEALTHY" = "true" ]; then
                printf 'INACTIVE|%s' "${CYAN}INACTIVE${NC}"
                return
            fi
            ;;
    esac

    case "$bucket" in
        aw-dlp-incidents|aw-dlp-review|aw-dlp-rules|aw-session-events)
            if [ "$age_sec" -lt 86400 ]; then
                printf 'FRESH|%s' "${GREEN}FRESH${NC}"
            else
                printf 'EVENT|%s' "${CYAN}EVENT-DRIVEN${NC}"
            fi
            ;;
        *)
            if [ "$age_sec" -lt 3600 ]; then
                printf 'FRESH|%s' "${GREEN}FRESH${NC}"
            elif [ "$age_sec" -lt 86400 ]; then
                printf 'STALE|%s' "${YELLOW}STALE${NC}"
            else
                printf 'DEAD|%s' "${RED}DEAD${NC}"
            fi
            ;;
    esac
}

classify_bucket_no_events() {
    local bucket="$1"

    case "$bucket" in
        aw-watcher-window)
            if [ "$HOST_INACTIVE" = "true" ]; then
                printf 'INACTIVE|%s' "${CYAN}INACTIVE${NC}"
                return
            fi
            ;;
        aw-dlp-endpoint-signals)
            if [ "$HOST_INACTIVE" = "true" ] && [ "$GUARD_HEALTHY" = "true" ]; then
                printf 'INACTIVE|%s' "${CYAN}INACTIVE${NC}"
                return
            fi
            ;;
        aw-dlp-incidents|aw-dlp-review|aw-dlp-rules|aw-session-events)
            printf 'EVENT|%s' "${CYAN}EVENT-DRIVEN${NC}"
            return
            ;;
    esac

    printf 'DEAD|%s' "${RED}EMPTY${NC}"
}

# Цвета
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}=== ActivityWatch Full Check: $HOSTNAME_FILTER ===${NC}"
echo ""

# 1. Проверка сервера
echo -e "${CYAN}--- 1. AW Server ($SERVER) ---${NC}"
echo -n "  Connectivity... "
RESP=$(no_proxy='*' curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/info" 2>&1)
if [ $? -eq 0 ] && echo "$RESP" | jq -e '.version' > /dev/null 2>&1; then
    VERSION=$(echo "$RESP" | jq -r '.version')
    echo -e "  ${GREEN}OK${NC} (aw-server $VERSION)"
else
    echo -e "  ${RED}FAILED${NC}"
    exit 1
fi

echo -n "  CORS... "
CORS_RESP=$(no_proxy='*' curl -s --connect-timeout 10 --max-time 15 -o /dev/null -w '%{http_code}' -H "Origin: $SERVER" "$SERVER/api/0/settings/" 2>&1)
if [ "$CORS_RESP" = "200" ]; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAIL${NC} (HTTP $CORS_RESP)"
fi
echo ""

# 1b. Context for inactive/event-driven classification
WORKTIME_EVENT_DATA=$(no_proxy='*' curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/buckets/aw-worktime-sessions_$HOSTNAME_FILTER/events?limit=1" 2>&1)
WORKTIME_TS=$(echo "$WORKTIME_EVENT_DATA" | jq -r '.[0].timestamp // ""' 2>/dev/null)
WORKTIME_ACTIVE=$(echo "$WORKTIME_EVENT_DATA" | jq -r '.[0].data.active // false' 2>/dev/null)
if [ -n "$WORKTIME_TS" ]; then
  WORKTIME_EPOCH=$(date -d "$WORKTIME_TS" +%s 2>/dev/null || echo 0)
  if [ "$WORKTIME_EPOCH" -gt 0 ]; then
    WORKTIME_AGE=$((NOW - WORKTIME_EPOCH))
    if [ "$WORKTIME_AGE" -lt 900 ] && [ "$WORKTIME_ACTIVE" != "true" ]; then
      HOST_INACTIVE=true
    fi
  fi
fi

GUARD_EVENT_DATA=$(no_proxy='*' curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/buckets/aw-rus-collector-guard_$HOSTNAME_FILTER/events?limit=1" 2>&1)
GUARD_TS=$(echo "$GUARD_EVENT_DATA" | jq -r '.[0].timestamp // ""' 2>/dev/null)
GUARD_STATUS=$(echo "$GUARD_EVENT_DATA" | jq -r '.[0].data.status // ""' 2>/dev/null)
GUARD_PROBLEMS=$(echo "$GUARD_EVENT_DATA" | jq -r '([.[0].data.problems[]?] | length) // 0' 2>/dev/null)
if [ -n "$GUARD_TS" ]; then
  GUARD_EPOCH=$(date -d "$GUARD_TS" +%s 2>/dev/null || echo 0)
  if [ "$GUARD_EPOCH" -gt 0 ]; then
    GUARD_AGE=$((NOW - GUARD_EPOCH))
    if [ "$GUARD_AGE" -lt 300 ] && [ "$GUARD_STATUS" = "ok" ] && [ "$GUARD_PROBLEMS" = "0" ]; then
      GUARD_HEALTHY=true
    fi
  fi
fi

# 2. Проверка бакетов
echo -e "${CYAN}--- 2. Data Buckets ---${NC}"
printf "  %-42s %-8s %-20s %s\n" "BUCKET" "EVENTS" "LAST EVENT" "STATUS"
printf "  %-42s %-8s %-20s %s\n" "------------------------------------------" "--------" "--------------------" "------"

BUCKETS=(
  "aw-watcher-afk|AFK watcher"
  "aw-watcher-window|Window watcher"
  "aw-worktime-sessions|Worktime sessions"
  "aw-session-events|Session events"
  "aw-dlp-endpoint-signals|DLP signals"
  "aw-dlp-incidents|DLP incidents"
  "aw-dlp-review|DLP review"
  "aw-dlp-rules|DLP rules"
)

for entry in "${BUCKETS[@]}"; do
  bucket="${entry%%|*}"
  label="${entry##*|}"
  if [ "$DLP_ENABLED" = "false" ] && [[ "$bucket" == aw-dlp-* ]]; then
    continue
  fi
  bucket_full="${bucket}_${HOSTNAME_FILTER}"
  
  EVENT_DATA=$(no_proxy='*' curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/buckets/$bucket_full/events?limit=1" 2>&1)
  LAST_ID=$(echo "$EVENT_DATA" | jq '.[0].id // 0')
  LAST_TS=$(echo "$EVENT_DATA" | jq -r '.[0].timestamp // "no events"')
  
  if [ "$LAST_TS" != "no events" ] && [ -n "$LAST_TS" ]; then
    EVENT_EPOCH=$(date -d "$LAST_TS" +%s 2>/dev/null || echo 0)
    if [ "$EVENT_EPOCH" -gt 0 ]; then
      AGE_SEC=$((NOW - EVENT_EPOCH))
      if [ $AGE_SEC -lt 3600 ]; then
        AGE="$((AGE_SEC / 60))m"
      elif [ $AGE_SEC -lt 86400 ]; then
        AGE="$((AGE_SEC / 3600))h"
      else
        AGE="$((AGE_SEC / 86400))d"
      fi
      CLASSIFICATION="$(classify_bucket_age "$bucket" "$AGE_SEC")"
      STATUS_KEY="${CLASSIFICATION%%|*}"
      STATUS="${CLASSIFICATION#*|}"
    else
      AGE="?"
      STATUS="${RED}?${NC}"
      STATUS_KEY="UNKNOWN"
    fi
  else
    AGE="none"
    LAST_ID="0"
    CLASSIFICATION="$(classify_bucket_no_events "$bucket")"
    STATUS_KEY="${CLASSIFICATION%%|*}"
    STATUS="${CLASSIFICATION#*|}"
  fi
  
  printf "  %-42s %-8s %-20s %b\n" "$label" "$LAST_ID" "$AGE" "$STATUS"
done
if [ "$DLP_ENABLED" = "false" ]; then
  printf "  %-42s %-8s %-20s %b\n" "DLP buckets" "-" "disabled" "${CYAN}SKIPPED${NC}"
fi
echo ""

# 3. Проверка RDP-хоста
echo -e "${CYAN}--- 3. RDP Host ($RDP_HOST) ---${NC}"

# Проверка WinRM
echo -n "  WinRM (5985)... "
if timeout 5 bash -c "echo > /dev/tcp/$RDP_HOST/5985" 2>&1; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}UNREACHABLE${NC}"
fi

# Проверка SSH (для справки)
echo -n "  SSH (22)... "
if timeout 5 bash -c "echo > /dev/tcp/$RDP_HOST/22" 2>&1; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${YELLOW}CLOSED${NC} (normal for Windows)"
fi
echo ""

# 4. Сводка
echo -e "${CYAN}--- 4. Summary ---${NC}"
FRESH_COUNT=0
STALE_COUNT=0
DEAD_COUNT=0

for entry in "${BUCKETS[@]}"; do
  bucket="${entry%%|*}"
  if [ "$DLP_ENABLED" = "false" ] && [[ "$bucket" == aw-dlp-* ]]; then
    continue
  fi
  bucket_full="${bucket}_${HOSTNAME_FILTER}"
  EVENT_DATA=$(no_proxy='*' curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/buckets/$bucket_full/events?limit=1" 2>&1)
  LAST_TS=$(echo "$EVENT_DATA" | jq -r '.[0].timestamp // "no events"')
  
  if [ "$LAST_TS" != "no events" ] && [ -n "$LAST_TS" ]; then
    EVENT_EPOCH=$(date -d "$LAST_TS" +%s 2>/dev/null || echo 0)
    if [ "$EVENT_EPOCH" -gt 0 ]; then
      AGE_SEC=$((NOW - EVENT_EPOCH))
      CLASSIFICATION="$(classify_bucket_age "$bucket" "$AGE_SEC")"
      STATUS_KEY="${CLASSIFICATION%%|*}"
      case "$STATUS_KEY" in
        FRESH|EVENT|INACTIVE)
          FRESH_COUNT=$((FRESH_COUNT + 1))
          ;;
        STALE)
          STALE_COUNT=$((STALE_COUNT + 1))
          ;;
        *)
          DEAD_COUNT=$((DEAD_COUNT + 1))
          ;;
      esac
    fi
  else
    CLASSIFICATION="$(classify_bucket_no_events "$bucket")"
    STATUS_KEY="${CLASSIFICATION%%|*}"
    case "$STATUS_KEY" in
      FRESH|EVENT|INACTIVE)
        FRESH_COUNT=$((FRESH_COUNT + 1))
        ;;
      STALE)
        STALE_COUNT=$((STALE_COUNT + 1))
        ;;
      *)
        DEAD_COUNT=$((DEAD_COUNT + 1))
        ;;
    esac
  fi
done

echo -e "  FRESH:  ${GREEN}$FRESH_COUNT${NC}"
echo -e "  STALE:  ${YELLOW}$STALE_COUNT${NC}"
echo -e "  DEAD:   ${RED}$DEAD_COUNT${NC}"

if [ $DEAD_COUNT -gt 0 ] || [ $STALE_COUNT -gt 0 ]; then
    echo ""
    echo -e "  ${RED}WARNING:${NC} Some collectors may need restart on RDP host"
    echo -e "  Run: ${CYAN}ansible -i ansible/inventory.ini rdp-prod -m win_shell -a 'schtasks /Run /TN \"ActivityWatch Recovery\"'${NC}"
fi

echo ""
echo -e "${CYAN}=== Check Complete ===${NC}"
echo "  Timestamp: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
