#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
STACK_DIR="${1:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ENV_FILE="$STACK_DIR/.env"

env_value() {
  key="$1"
  default="$2"
  current=$(eval "printf '%s' \"\${$key:-}\"")
  if [ -n "$current" ]; then
    printf '%s' "$current"
    return
  fi
  if [ -f "$ENV_FILE" ]; then
    value=$(sed -n "s/^$key=//p" "$ENV_FILE" | tail -n 1)
    if [ -n "$value" ]; then
      printf '%s' "$value"
      return
    fi
  fi
  printf '%s' "$default"
}

GRAFANA_PORT=$(env_value GRAFANA_PORT 3000)
PROMETHEUS_PORT=$(env_value PROMETHEUS_PORT 9090)
SQL_EXPORTER_PORT=$(env_value SQL_EXPORTER_PORT 9399)
AW_EXPORTER_PORT=$(env_value AW_EXPORTER_PORT 9398)
GRAFANA_ADMIN_USER=$(env_value GRAFANA_ADMIN_USER admin)
GRAFANA_ADMIN_PASSWORD=$(env_value GRAFANA_ADMIN_PASSWORD change_me_now)

TMP_DIR="${TMPDIR:-/tmp}"
METRICS_OUT="$TMP_DIR/awrus-onec-metrics.out"
AW_METRICS_OUT="$TMP_DIR/awrus-aw-metrics.out"
PROM_HEALTH_OUT="$TMP_DIR/awrus-prom-healthy.out"
PROM_UP_OUT="$TMP_DIR/awrus-prom-up.json"
PROM_FRESHNESS_OUT="$TMP_DIR/awrus-prom-freshness.json"
GRAFANA_HEALTH_OUT="$TMP_DIR/awrus-grafana-health.json"
GRAFANA_DS_OUT="$TMP_DIR/awrus-grafana-datasources.json"
GRAFANA_DASH_OUT="$TMP_DIR/awrus-grafana-dashboards.json"

require_metric() {
  metric_name="$1"
  metrics_file="$2"
  if ! grep -q "^$metric_name" "$metrics_file"; then
    echo "[!] Required metric '$metric_name' was not found in $metrics_file" >&2
    exit 1
  fi
}

require_prometheus_success() {
  file="$1"
  if ! grep -q '"status":"success"' "$file"; then
    echo "[!] Prometheus query did not return status=success: $file" >&2
    cat "$file" >&2
    exit 1
  fi
}

echo "[*] Checking exporter endpoints"
curl -fsS "http://127.0.0.1:$SQL_EXPORTER_PORT/metrics" >"$METRICS_OUT"
curl -fsS "http://127.0.0.1:$AW_EXPORTER_PORT/metrics" >"$AW_METRICS_OUT"
require_metric "onec_data_freshness_seconds" "$METRICS_OUT"
require_metric "aw_up" "$AW_METRICS_OUT"

echo "[*] Checking Prometheus health and scrape targets"
curl -fsS "http://127.0.0.1:$PROMETHEUS_PORT/-/healthy" >"$PROM_HEALTH_OUT"
curl -fsS "http://127.0.0.1:$PROMETHEUS_PORT/api/v1/query?query=up%7Bjob%3D~%22onec_sql_exporter%7Caw_activitywatch_exporter%22%7D" >"$PROM_UP_OUT"
curl -fsS "http://127.0.0.1:$PROMETHEUS_PORT/api/v1/query?query=onec_data_freshness_seconds" >"$PROM_FRESHNESS_OUT"
require_prometheus_success "$PROM_UP_OUT"
require_prometheus_success "$PROM_FRESHNESS_OUT"

echo "[*] Checking Grafana health, datasource and dashboards"
curl -fsS "http://127.0.0.1:$GRAFANA_PORT/api/health" >"$GRAFANA_HEALTH_OUT"
curl -fsS -u "$GRAFANA_ADMIN_USER:$GRAFANA_ADMIN_PASSWORD" "http://127.0.0.1:$GRAFANA_PORT/api/datasources/uid/prometheus" >"$GRAFANA_DS_OUT"
curl -fsS -u "$GRAFANA_ADMIN_USER:$GRAFANA_ADMIN_PASSWORD" "http://127.0.0.1:$GRAFANA_PORT/api/search?type=dash-db&query=" >"$GRAFANA_DASH_OUT"

if command -v docker >/dev/null 2>&1; then
  echo "[*] Checking container status"
  cd "$STACK_DIR"
  docker compose ps
else
  echo "[*] docker command not found; skipping container status"
fi

echo "[+] Pipeline health artifacts:"
echo "    $METRICS_OUT"
echo "    $AW_METRICS_OUT"
echo "    $PROM_HEALTH_OUT"
echo "    $PROM_UP_OUT"
echo "    $PROM_FRESHNESS_OUT"
echo "    $GRAFANA_HEALTH_OUT"
echo "    $GRAFANA_DS_OUT"
echo "    $GRAFANA_DASH_OUT"
