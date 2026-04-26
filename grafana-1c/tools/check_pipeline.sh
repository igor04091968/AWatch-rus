#!/bin/sh
set -eu

STACK_DIR="${1:-/mnt/usb_hdd2/Projects/ActivityWatch-Russian/grafana-1c}"

echo "[*] Checking endpoints"
curl -fsS http://127.0.0.1:9399/metrics >/tmp/awrus-onec-metrics.out
curl -fsS http://127.0.0.1:9090/-/healthy >/tmp/awrus-prom-healthy.out
curl -fsS "http://127.0.0.1:9090/api/v1/query?query=up%7Bjob%3D%22onec_sql_exporter%22%7D" >/tmp/awrus-prom-up.json
curl -fsS "http://127.0.0.1:9090/api/v1/query?query=onec_data_freshness_seconds" >/tmp/awrus-prom-freshness.json

echo "[*] Checking container status"
cd "$STACK_DIR"
docker compose ps

echo "[+] Pipeline health artifacts:"
echo "    /tmp/awrus-onec-metrics.out"
echo "    /tmp/awrus-prom-healthy.out"
echo "    /tmp/awrus-prom-up.json"
echo "    /tmp/awrus-prom-freshness.json"
