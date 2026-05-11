#!/bin/bash
set -euo pipefail

# Health check script for AW services
# Returns 0 if all services are healthy, 1 otherwise

SERVICES=("activitywatch-server" "aw-worktime-api" "aw-worktime-ui-bridge")
UNHEALTHY_SERVICES=()
WARNINGS=()

check_service() {
    local service=$1
    if systemctl is-active --quiet "$service"; then
        echo "✓ $service is running"
    else
        echo "✗ $service is not running"
        UNHEALTHY_SERVICES+=("$service")
    fi
}

check_api_endpoint() {
    local url=$1
    local service_name=$2
    
    if curl -s --max-time 5 "$url" >/dev/null 2>&1; then
        echo "✓ $service_name API endpoint is responding"
    else
        echo "✗ $service_name API endpoint is not responding"
        UNHEALTHY_SERVICES+=("$service_name-api")
    fi
}

check_dlp_transport_freshness() {
    local api_base="${1:-http://127.0.0.1:5600/api/0}"
    local max_age_seconds="${2:-900}"
    local result

    if ! command -v python3 >/dev/null 2>&1; then
        echo "⚠ python3 is not available, skipping DLP transport freshness checks"
        WARNINGS+=("dlp-transport-check-skipped")
        return
    fi

    result="$(python3 - "$api_base" "$max_age_seconds" <<'PY'
import json
import sys
import time
from urllib.request import urlopen

api_base = sys.argv[1].rstrip("/")
max_age = int(sys.argv[2])
now = time.time()

def parse_ts(ts):
    if not ts:
        return None
    ts = ts.replace("Z", "+00:00")
    try:
        from datetime import datetime
        return datetime.fromisoformat(ts).timestamp()
    except Exception:
        return None

def get_json(url):
    with urlopen(url, timeout=8) as resp:
        return json.loads(resp.read().decode("utf-8"))

out = {
    "ok": True,
    "warnings": [],
    "errors": []
}

try:
    buckets = get_json(f"{api_base}/buckets/")
except Exception as ex:
    out["ok"] = False
    out["errors"].append(f"dlp-buckets-read-failed:{ex}")
    print(json.dumps(out))
    sys.exit(0)

endpoint = [k for k in buckets.keys() if k.startswith("aw-dlp-endpoint-signals_")]
fileops = [k for k in buckets.keys() if k.startswith("aw-file-operations_")]

if not endpoint:
    out["ok"] = False
    out["errors"].append("no-endpoint-signal-buckets")
if not fileops:
    out["warnings"].append("no-file-operations-buckets")

def check_bucket_freshness(bucket_id, label):
    b = buckets.get(bucket_id, {})
    meta = b.get("metadata") or {}
    end = parse_ts(meta.get("end"))
    if end is None:
        # Some aw-server deployments may not populate metadata.end; fallback to latest event.
        try:
            events = get_json(f"{api_base}/buckets/{bucket_id}/events?limit=1")
            if events:
                end = parse_ts(events[0].get("timestamp"))
        except Exception:
            end = None
    if end is None:
        out["warnings"].append(f"{label}:no-end-ts-or-events:{bucket_id}")
        return
    age = int(now - end)
    if age > max_age:
        out["ok"] = False
        out["errors"].append(f"{label}:stale:{bucket_id}:age={age}s")

for bid in endpoint:
    check_bucket_freshness(bid, "endpoint")
for bid in fileops:
    check_bucket_freshness(bid, "fileops")

# Validate that endpoint self_test contains transport metrics at least once recently.
for bid in endpoint:
    try:
        events = get_json(f"{api_base}/buckets/{bid}/events?limit=20")
        found = False
        for e in events:
            d = e.get("data") or {}
            if d.get("signalType") == "self_test":
                if all(k in d for k in ("queueDepth", "eventsEnqueued", "eventsFlushed", "sendFailures")):
                    found = True
                    break
        if not found:
            out["warnings"].append(f"endpoint:self_test-metrics-missing:{bid}")
    except Exception as ex:
        out["warnings"].append(f"endpoint:self_test-read-failed:{bid}:{ex}")

print(json.dumps(out))
PY
)" || true

    local ok
    ok="$(printf '%s' "$result" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("1" if d.get("ok") else "0")' 2>/dev/null || echo "0")"
    if [[ "$ok" == "1" ]]; then
        echo "✓ DLP transport freshness check passed"
    else
        echo "✗ DLP transport freshness check failed"
        UNHEALTHY_SERVICES+=("dlp-transport")
    fi

    local errors warnings
    errors="$(printf '%s' "$result" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(", ".join(d.get("errors", [])))' 2>/dev/null || true)"
    warnings="$(printf '%s' "$result" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(", ".join(d.get("warnings", [])))' 2>/dev/null || true)"
    if [[ -n "$errors" ]]; then
        echo "  errors: $errors"
    fi
    if [[ -n "$warnings" ]]; then
        echo "  warnings: $warnings"
        WARNINGS+=("$warnings")
    fi
}

echo "=== AW Services Health Check ==="
echo "Timestamp: $(date)"
echo

# Check systemd services
for service in "${SERVICES[@]}"; do
    check_service "$service"
done

echo

# Check API endpoints
check_api_endpoint "http://127.0.0.1:5600/api/0/info" "activitywatch-server"
check_api_endpoint "http://127.0.0.1:5610/reports/worktime/today" "aw-worktime-api"
check_dlp_transport_freshness "http://127.0.0.1:5600/api/0" "900"

echo

if [ ${#UNHEALTHY_SERVICES[@]} -eq 0 ]; then
    echo "✓ All services are healthy"
    if [ ${#WARNINGS[@]} -gt 0 ]; then
        echo "⚠ Warnings: ${WARNINGS[*]}"
    fi
    exit 0
else
    echo "✗ Unhealthy services: ${UNHEALTHY_SERVICES[*]}"
    exit 1
fi
