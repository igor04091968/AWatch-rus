#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

CLICKHOUSE_CONTAINER="${CLICKHOUSE_CONTAINER:-aw-rus-workforce-clickhouse}"
CLICKHOUSE_DATABASE="${CLICKHOUSE_DATABASE:-aw_workforce}"
CLICKHOUSE_CLIENT_BIN="${CLICKHOUSE_CLIENT_BIN:-clickhouse-client}"
LIMIT="${LIMIT:-50}"

run_query() {
  local query="$1"

  if docker ps --format '{{.Names}}' | grep -Fxq "$CLICKHOUSE_CONTAINER"; then
    docker exec -i "$CLICKHOUSE_CONTAINER" clickhouse-client \
      --database "$CLICKHOUSE_DATABASE" --query "$query"
    return
  fi

  "$CLICKHOUSE_CLIENT_BIN" --database "$CLICKHOUSE_DATABASE" --query "$query"
}

cd "$ROOT_DIR"

printf '\n[unknown subjects]\n'
run_query "
SELECT *
FROM $CLICKHOUSE_DATABASE.v_workforce_unknown_subjects
LIMIT $LIMIT
FORMAT PrettyCompact
"

printf '\n[unknown processes]\n'
run_query "
SELECT *
FROM $CLICKHOUSE_DATABASE.v_workforce_unknown_processes
LIMIT $LIMIT
FORMAT PrettyCompact
"

printf '\n[unknown domains]\n'
run_query "
SELECT *
FROM $CLICKHOUSE_DATABASE.v_workforce_unknown_domains
LIMIT $LIMIT
FORMAT PrettyCompact
"
