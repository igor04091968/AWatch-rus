#!/bin/bash
# check-aw-data.sh - Проверка сбора данных ActivityWatch с RDP-сервера SHARKON2025
# Сервер: 10.10.10.13:5600
# Хост-источник: 192.168.100.18 (SHARKON2025)

SERVER="http://10.10.10.13:5600"
HOSTNAME_FILTER="SHARKON2025"
NOW=$(date -u +%s)

# Цвета
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Проверка доступности сервера
echo "=== ActivityWatch Data Check: $HOSTNAME_FILTER ==="
echo ""
echo -n "Server connectivity... "
RESP=$(no_proxy=10.10.10.13 curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/info" 2>&1)
if [ $? -eq 0 ] && echo "$RESP" | jq -e '.version' > /dev/null 2>&1; then
    VERSION=$(echo "$RESP" | jq -r '.version')
    echo -e "${GREEN}OK${NC} (aw-server v$VERSION)"
else
    echo -e "${RED}FAILED${NC} (cannot reach $SERVER)"
    exit 1
fi
echo ""

# Проверка бакетов
echo "--- Buckets ---"
printf "%-45s %-8s %-22s %s\n" "BUCKET" "EVENTS" "LAST EVENT" "STATUS"
printf "%-45s %-8s %-22s %s\n" "---------------------------------------------" "--------" "----------------------" "------"

BUCKETS=(
  "aw-dlp-endpoint-signals"
  "aw-dlp-incidents"
  "aw-dlp-review"
  "aw-dlp-rules"
  "aw-watcher-afk"
  "aw-watcher-window"
  "aw-session-events"
  "aw-worktime-sessions"
)

for bucket in "${BUCKETS[@]}"; do
  bucket_full="${bucket}_${HOSTNAME_FILTER}"
  
  # Получаем последний event
  EVENT_DATA=$(no_proxy=10.10.10.13 curl -s --connect-timeout 10 --max-time 15 "$SERVER/api/0/buckets/$bucket_full/events?limit=1" 2>&1)
  LAST_ID=$(echo "$EVENT_DATA" | jq '.[0].id // 0')
  LAST_TS=$(echo "$EVENT_DATA" | jq -r '.[0].timestamp // "no events"')
  
  # Вычисляем возраст
  if [ "$LAST_TS" != "no events" ] && [ -n "$LAST_TS" ]; then
    EVENT_EPOCH=$(date -d "$LAST_TS" +%s 2>/dev/null || echo 0)
    if [ "$EVENT_EPOCH" -gt 0 ]; then
      AGE_SEC=$((NOW - EVENT_EPOCH))
      if [ $AGE_SEC -lt 3600 ]; then
        AGE="$((AGE_SEC / 60))m ago"
        STATUS="${GREEN}FRESH${NC}"
      elif [ $AGE_SEC -lt 86400 ]; then
        AGE="$((AGE_SEC / 3600))h ago"
        STATUS="${YELLOW}STALE${NC}"
      else
        AGE="$((AGE_SEC / 86400))d ago"
        STATUS="${RED}DEAD${NC}"
      fi
    else
      AGE="unknown"
      STATUS="${RED}?${NC}"
    fi
  else
    AGE="none"
    LAST_ID="0"
    STATUS="${RED}EMPTY${NC}"
  fi
  
  printf "%-45s %-8s %-22s %b\n" "$bucket_full" "$LAST_ID" "$LAST_TS ($AGE)" "$STATUS"
done

echo ""

# Проверка CORS
echo "--- CORS Check ---"
CORS_RESP=$(no_proxy=10.10.10.13 curl -s --connect-timeout 10 --max-time 15 -o /dev/null -w '%{http_code}' -H "Origin: http://10.10.10.13:5600" "$SERVER/api/0/settings/" 2>&1)
if [ "$CORS_RESP" = "200" ]; then
    echo -e "${GREEN}CORS: OK${NC} (HTTP 200)"
else
    echo -e "${RED}CORS: FAIL${NC} (HTTP $CORS_RESP)"
fi

echo ""
echo "=== Check Complete ==="
echo "Timestamp: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
