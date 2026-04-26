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
OUT_DIR="${2:-/tmp/onec-schema-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$OUT_DIR"

export PGPASSWORD=""

echo "[*] Dumping schemas, tables, columns, candidate accounting objects into $OUT_DIR"

psql "$DSN" -X -v ON_ERROR_STOP=1 -c "\dn+" >"$OUT_DIR/schemas.txt"
psql "$DSN" -X -v ON_ERROR_STOP=1 -c "\dt+ *.*" >"$OUT_DIR/tables.txt"
psql "$DSN" -X -v ON_ERROR_STOP=1 -c "
SELECT table_schema, table_name, column_name, data_type
FROM information_schema.columns
ORDER BY table_schema, table_name, ordinal_position;
" >"$OUT_DIR/columns.tsv"

psql "$DSN" -X -v ON_ERROR_STOP=1 -c "
SELECT table_schema, table_name
FROM information_schema.tables
WHERE table_type='BASE TABLE'
  AND (
    table_name ILIKE '%doc%' OR
    table_name ILIKE '%sale%' OR
    table_name ILIKE '%realiz%' OR
    table_name ILIKE '%debt%' OR
    table_name ILIKE '%debitor%' OR
    table_name ILIKE '%error%' OR
    table_name ILIKE '%post%'
  )
ORDER BY table_schema, table_name;
" >"$OUT_DIR/candidate_tables.tsv"

echo "[+] Done: $OUT_DIR"
