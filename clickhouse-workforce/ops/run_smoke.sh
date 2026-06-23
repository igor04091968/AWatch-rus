#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

CLICKHOUSE_CONTAINER="${CLICKHOUSE_CONTAINER:-aw-rus-workforce-clickhouse}"
CLICKHOUSE_DATABASE="${CLICKHOUSE_DATABASE:-aw_workforce}"
CLICKHOUSE_USER="${CLICKHOUSE_USER:-}"
CLICKHOUSE_PASSWORD="${CLICKHOUSE_PASSWORD:-}"
CLICKHOUSE_CLIENT_BIN="${CLICKHOUSE_CLIENT_BIN:-clickhouse-client}"
LOAD_DEMO_SEED="${LOAD_DEMO_SEED:-1}"
CLICKHOUSE_READY_TIMEOUT_SEC="${CLICKHOUSE_READY_TIMEOUT_SEC:-60}"

client_auth_args=()
if [[ -n "$CLICKHOUSE_USER" ]]; then
  client_auth_args+=(--user "$CLICKHOUSE_USER")
fi
if [[ -n "$CLICKHOUSE_PASSWORD" ]]; then
  client_auth_args+=(--password "$CLICKHOUSE_PASSWORD")
fi

run_client() {
  local query_file="${1:-}"
  local query="${2:-}"

  if docker ps --format '{{.Names}}' | grep -Fxq "$CLICKHOUSE_CONTAINER"; then
    if [[ -n "$query_file" ]]; then
      docker exec -i "$CLICKHOUSE_CONTAINER" clickhouse-client "${client_auth_args[@]}" --multiquery <"$query_file"
    else
      docker exec -i "$CLICKHOUSE_CONTAINER" clickhouse-client "${client_auth_args[@]}" --database "$CLICKHOUSE_DATABASE" --query "$query"
    fi
    return
  fi

  if command -v "$CLICKHOUSE_CLIENT_BIN" >/dev/null 2>&1; then
    if [[ -n "$query_file" ]]; then
      "$CLICKHOUSE_CLIENT_BIN" "${client_auth_args[@]}" --multiquery <"$query_file"
    else
      "$CLICKHOUSE_CLIENT_BIN" "${client_auth_args[@]}" --database "$CLICKHOUSE_DATABASE" --query "$query"
    fi
    return
  fi

  printf 'No running ClickHouse container "%s" and no %s in PATH\n' \
    "$CLICKHOUSE_CONTAINER" "$CLICKHOUSE_CLIENT_BIN" >&2
  return 127
}

wait_for_clickhouse() {
  local deadline
  deadline=$((SECONDS + CLICKHOUSE_READY_TIMEOUT_SEC))
  while (( SECONDS < deadline )); do
    if run_client "" "SELECT 1" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  printf 'ClickHouse is not ready after %s seconds\n' "$CLICKHOUSE_READY_TIMEOUT_SEC" >&2
  return 1
}

apply_sql_dir() {
  local sql_file
  for sql_file in "$ROOT_DIR"/clickhouse/init/*.sql; do
    printf '[sql] %s\n' "${sql_file#$ROOT_DIR/}"
    run_client "$sql_file" ""
  done
}

assert_scalar_nonzero() {
  local name="$1"
  local query="$2"
  local value
  value="$(run_client "" "$query" | tr -d '[:space:]')"
  if [[ ! "$value" =~ ^[0-9]+$ ]] || (( value < 1 )); then
    printf '[FAIL] %s: expected positive integer, got "%s"\n' "$name" "$value" >&2
    return 1
  fi
  printf '[OK]   %s: %s\n' "$name" "$value"
}

assert_no_dictionary_errors() {
  local errors
  errors="$(run_client "" "
SELECT count()
FROM system.dictionaries
WHERE database = '$CLICKHOUSE_DATABASE'
  AND name IN ('dict_workstation_user', 'dict_application_category', 'dict_domain_category')
  AND (status != 'LOADED' OR last_exception != '')
")"
  errors="$(printf '%s' "$errors" | tr -d '[:space:]')"
  if [[ "$errors" != "0" ]]; then
    printf '[FAIL] dictionaries have load errors\n' >&2
    run_client "" "
SELECT database, name, status, last_exception
FROM system.dictionaries
WHERE database = '$CLICKHOUSE_DATABASE'
  AND name IN ('dict_workstation_user', 'dict_application_category', 'dict_domain_category')
FORMAT Vertical
"
    return 1
  fi
  printf '[OK]   dictionaries loaded\n'
}

wait_for_clickhouse
apply_sql_dir

if [[ "$LOAD_DEMO_SEED" == "1" ]]; then
  printf '[sql] sample/seed_demo.sql\n'
  run_client "$ROOT_DIR/sample/seed_demo.sql" ""
fi

assert_no_dictionary_errors
assert_scalar_nonzero "dictionary count" "
SELECT count()
FROM system.dictionaries
WHERE database = '$CLICKHOUSE_DATABASE'
  AND name IN ('dict_workstation_user', 'dict_application_category', 'dict_domain_category')
"
assert_scalar_nonzero "hourly aggregate rows" "
SELECT count()
FROM $CLICKHOUSE_DATABASE.agg_workforce_productivity_hourly
"
assert_scalar_nonzero "daily productivity rows" "
SELECT count()
FROM $CLICKHOUSE_DATABASE.v_workforce_productivity_daily
"
assert_scalar_nonzero "unknown quality rows" "
SELECT count()
FROM $CLICKHOUSE_DATABASE.v_workforce_unknown_quality_daily
"

printf '[OK] ClickHouse workforce smoke completed\n'
