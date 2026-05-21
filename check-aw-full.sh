#!/bin/bash
# check-aw-full.sh - Полная проверка ActivityWatch: сервер + RDP-хост
# Сервер: 10.10.10.13:5600
# RDP-хост: 192.168.100.18 (SHARKON2025)

SERVER="http://10.10.10.13:5600"
HOSTNAME_FILTER="SHARKON2025"
RDP_HOST="192.168.100.18"
NOW=$(date -u +%s)

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
RESP=$(no_proxy=10.10.10.13 curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/info" 2>&1)
if [ $? -eq 0 ] && echo "$RESP" | jq -e '.version' > /dev/null 2>&1; then
    VERSION=$(echo "$RESP" | jq -r '.version')
    echo -e "  ${GREEN}OK${NC} (aw-server $VERSION)"
else
    echo -e "  ${RED}FAILED${NC}"
    exit 1
fi

echo -n "  CORS... "
CORS_RESP=$(no_proxy=10.10.10.13 curl -s --connect-timeout 10 --max-time 15 -o /dev/null -w '%{http_code}' -H "Origin: http://10.10.10.13:5600" "$SERVER/api/0/settings/" 2>&1)
if [ "$CORS_RESP" = "200" ]; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAIL${NC} (HTTP $CORS_RESP)"
fi
echo ""

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
  bucket_full="${bucket}_${HOSTNAME_FILTER}"
  
  EVENT_DATA=$(no_proxy=10.10.10.13 curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/buckets/$bucket_full/events?limit=1" 2>&1)
  LAST_ID=$(echo "$EVENT_DATA" | jq '.[0].id // 0')
  LAST_TS=$(echo "$EVENT_DATA" | jq -r '.[0].timestamp // "no events"')
  
  if [ "$LAST_TS" != "no events" ] && [ -n "$LAST_TS" ]; then
    EVENT_EPOCH=$(date -d "$LAST_TS" +%s 2>/dev/null || echo 0)
    if [ "$EVENT_EPOCH" -gt 0 ]; then
      AGE_SEC=$((NOW - EVENT_EPOCH))
      if [ $AGE_SEC -lt 3600 ]; then
        AGE="$((AGE_SEC / 60))m"
        STATUS="${GREEN}FRESH${NC}"
      elif [ $AGE_SEC -lt 86400 ]; then
        AGE="$((AGE_SEC / 3600))h"
        STATUS="${YELLOW}STALE${NC}"
      else
        AGE="$((AGE_SEC / 86400))d"
        STATUS="${RED}DEAD${NC}"
      fi
    else
      AGE="?"
      STATUS="${RED}?${NC}"
    fi
  else
    AGE="none"
    LAST_ID="0"
    STATUS="${RED}EMPTY${NC}"
  fi
  
  printf "  %-42s %-8s %-20s %b\n" "$label" "$LAST_ID" "$AGE" "$STATUS"
done
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
  bucket_full="${bucket}_${HOSTNAME_FILTER}"
  EVENT_DATA=$(no_proxy=10.10.10.13 curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/buckets/$bucket_full/events?limit=1" 2>&1)
  LAST_TS=$(echo "$EVENT_DATA" | jq -r '.[0].timestamp // "no events"')
  
  if [ "$LAST_TS" != "no events" ] && [ -n "$LAST_TS" ]; then
    EVENT_EPOCH=$(date -d "$LAST_TS" +%s 2>/dev/null || echo 0)
    if [ "$EVENT_EPOCH" -gt 0 ]; then
      AGE_SEC=$((NOW - EVENT_EPOCH))
      if [ $AGE_SEC -lt 3600 ]; then
        FRESH_COUNT=$((FRESH_COUNT + 1))
      elif [ $AGE_SEC -lt 86400 ]; then
        STALE_COUNT=$((STALE_COUNT + 1))
      else
        DEAD_COUNT=$((DEAD_COUNT + 1))
      fi
    fi
  else
    DEAD_COUNT=$((DEAD_COUNT + 1))
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
