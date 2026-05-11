#!/bin/bash
set -euo pipefail

# Health check script for AW services
# Returns 0 if all services are healthy, 1 otherwise

SERVICES=("activitywatch-server" "aw-worktime-api" "aw-worktime-ui-bridge")
UNHEALTHY_SERVICES=()

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

echo

if [ ${#UNHEALTHY_SERVICES[@]} -eq 0 ]; then
    echo "✓ All services are healthy"
    exit 0
else
    echo "✗ Unhealthy services: ${UNHEALTHY_SERVICES[*]}"
    exit 1
fi