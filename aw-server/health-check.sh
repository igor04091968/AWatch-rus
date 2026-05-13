#!/bin/bash
set -euo pipefail

# Health check script for AW services
# Returns 0 if all services are healthy, 1 otherwise

SERVICES=("activitywatch-server" "aw-worktime-api" "aw-worktime-ui-bridge")
UNHEALTHY_SERVICES=()
WARNINGS=()

check_service() {
    local service=$1
    if [[ "$service" == "aw-worktime-ui-bridge" ]]; then
        if systemctl is-active --quiet aw-worktime-ui-bridge.timer && systemctl is-enabled --quiet aw-worktime-ui-bridge.timer; then
            echo "✓ aw-worktime-ui-bridge.timer is running and enabled"
        else
            echo "✗ aw-worktime-ui-bridge.timer is not active/enabled"
            UNHEALTHY_SERVICES+=("aw-worktime-ui-bridge.timer")
        fi
        return
    fi
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
    local dlp_health="${DLP_HEALTH_BIN:-/usr/local/bin/dlp-health-check}"
    local result

    if [[ ! -x "$dlp_health" ]]; then
        echo "⚠ dlp-health-check is not available, skipping DLP transport freshness checks"
        WARNINGS+=("dlp-health-check-missing")
        return
    fi

    result="$("$dlp_health" --json 2>/dev/null || true)"
    if [[ -z "$result" ]]; then
        echo "⚠ dlp-health-check did not return JSON, skipping DLP transport freshness checks"
        WARNINGS+=("dlp-health-check-empty")
        return
    fi

    local ok
    ok="$(printf '%s' "$result" | python3 -c 'import json,sys; data=json.load(sys.stdin); names={r["name"]:r for r in data.get("results", [])}; checks=["buckets:endpoint-signals","buckets:file-operations","endpoint-self-test-metrics"]; bad=[n for n in checks if names.get(n,{}).get("status")=="fail"]; print("1" if not bad else "0")' 2>/dev/null || echo "0")"
    if [[ "$ok" == "1" ]]; then
        echo "✓ DLP transport freshness check passed"
    else
        echo "✗ DLP transport freshness check failed"
        UNHEALTHY_SERVICES+=("dlp-transport")
    fi

    local errors warnings
    errors="$(printf '%s' "$result" | python3 -c 'import json,sys; data=json.load(sys.stdin); out=[]; [out.append(f"{r.get(\"name\")}:{r.get(\"summary\")}") for r in data.get("results", []) if r.get("status")=="fail" and r.get("name") in ("buckets:endpoint-signals","buckets:file-operations","endpoint-self-test-metrics")]; print(", ".join(out))' 2>/dev/null || true)"
    warnings="$(printf '%s' "$result" | python3 -c 'import json,sys; data=json.load(sys.stdin); out=[]; [out.append(f"{r.get(\"name\")}:{r.get(\"summary\")}") for r in data.get("results", []) if r.get("status")=="warn" and r.get("name") in ("buckets:endpoint-signals","buckets:file-operations","endpoint-self-test-metrics")]; print(", ".join(out))' 2>/dev/null || true)"
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
check_dlp_transport_freshness "http://127.0.0.1:5600/api/0" "900" "${AW_HEALTH_STRICT_FILEOPS:-0}"

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
