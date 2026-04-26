#!/bin/sh
set -eu

if ! command -v psql >/dev/null 2>&1; then
  echo "psql not found" >&2
  exit 1
fi

if [ "${1:-}" = "" ]; then
  echo "Usage: $0 \"postgres://user:pass@host:5432/db?sslmode=disable\"" >&2
  exit 1
fi

DSN="$1"

VIEWS="
onec_kpi_unposted_documents
onec_kpi_sales_today
onec_kpi_overdue_receivables
onec_kpi_posting_errors_24h
onec_kpi_data_freshness
"

echo "[*] Checking required KPI views"
for view_name in $VIEWS; do
  exists="$(psql "$DSN" -X -A -t -v ON_ERROR_STOP=1 -c "SELECT EXISTS (
      SELECT 1
      FROM information_schema.views
      WHERE table_name='${view_name}'
  );")"
  if [ "$exists" != "t" ]; then
    echo "[-] Missing view: $view_name" >&2
    exit 2
  fi
done

echo "[*] Running test queries"
psql "$DSN" -X -v ON_ERROR_STOP=1 -c "SELECT * FROM onec_kpi_unposted_documents LIMIT 5;"
psql "$DSN" -X -v ON_ERROR_STOP=1 -c "SELECT * FROM onec_kpi_sales_today LIMIT 5;"
psql "$DSN" -X -v ON_ERROR_STOP=1 -c "SELECT * FROM onec_kpi_overdue_receivables LIMIT 5;"
psql "$DSN" -X -v ON_ERROR_STOP=1 -c "SELECT * FROM onec_kpi_posting_errors_24h LIMIT 5;"
psql "$DSN" -X -v ON_ERROR_STOP=1 -c "SELECT * FROM onec_kpi_data_freshness LIMIT 5;"

echo "[+] KPI views are valid"
