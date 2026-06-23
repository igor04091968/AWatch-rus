#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CATALOG_DIR="${CATALOG_DIR:-$ROOT_DIR/catalog}"

CLICKHOUSE_CONTAINER="${CLICKHOUSE_CONTAINER:-aw-rus-workforce-clickhouse}"
CLICKHOUSE_DATABASE="${CLICKHOUSE_DATABASE:-aw_workforce}"
CLICKHOUSE_USER="${CLICKHOUSE_USER:-}"
CLICKHOUSE_PASSWORD="${CLICKHOUSE_PASSWORD:-}"
CLICKHOUSE_CLIENT_BIN="${CLICKHOUSE_CLIENT_BIN:-clickhouse-client}"
CLICKHOUSE_READY_TIMEOUT_SEC="${CLICKHOUSE_READY_TIMEOUT_SEC:-60}"
REBUILD_AGGREGATES="${REBUILD_AGGREGATES:-0}"

client_auth_args=()
if [[ -n "$CLICKHOUSE_USER" ]]; then
  client_auth_args+=(--user "$CLICKHOUSE_USER")
fi
if [[ -n "$CLICKHOUSE_PASSWORD" ]]; then
  client_auth_args+=(--password "$CLICKHOUSE_PASSWORD")
fi

run_query() {
  local query="$1"

  if docker ps --format '{{.Names}}' | grep -Fxq "$CLICKHOUSE_CONTAINER"; then
    docker exec -i "$CLICKHOUSE_CONTAINER" clickhouse-client "${client_auth_args[@]}" \
      --database "$CLICKHOUSE_DATABASE" --query "$query"
    return
  fi

  if command -v "$CLICKHOUSE_CLIENT_BIN" >/dev/null 2>&1; then
    "$CLICKHOUSE_CLIENT_BIN" "${client_auth_args[@]}" \
      --database "$CLICKHOUSE_DATABASE" --query "$query"
    return
  fi

  printf 'No running ClickHouse container "%s" and no %s in PATH\n' \
    "$CLICKHOUSE_CONTAINER" "$CLICKHOUSE_CLIENT_BIN" >&2
  return 127
}

run_query_file() {
  local query_file="$1"

  if docker ps --format '{{.Names}}' | grep -Fxq "$CLICKHOUSE_CONTAINER"; then
    docker exec -i "$CLICKHOUSE_CONTAINER" clickhouse-client "${client_auth_args[@]}" \
      --multiquery <"$query_file"
    return
  fi

  if command -v "$CLICKHOUSE_CLIENT_BIN" >/dev/null 2>&1; then
    "$CLICKHOUSE_CLIENT_BIN" "${client_auth_args[@]}" --multiquery <"$query_file"
    return
  fi

  printf 'No running ClickHouse container "%s" and no %s in PATH\n' \
    "$CLICKHOUSE_CONTAINER" "$CLICKHOUSE_CLIENT_BIN" >&2
  return 127
}

run_insert_file() {
  local query="$1"
  local data_file="$2"

  if docker ps --format '{{.Names}}' | grep -Fxq "$CLICKHOUSE_CONTAINER"; then
    docker exec -i "$CLICKHOUSE_CONTAINER" clickhouse-client "${client_auth_args[@]}" \
      --database "$CLICKHOUSE_DATABASE" --query "$query" <"$data_file"
    return
  fi

  if command -v "$CLICKHOUSE_CLIENT_BIN" >/dev/null 2>&1; then
    "$CLICKHOUSE_CLIENT_BIN" "${client_auth_args[@]}" \
      --database "$CLICKHOUSE_DATABASE" --query "$query" <"$data_file"
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
    if run_query "SELECT 1" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  printf 'ClickHouse is not ready after %s seconds\n' "$CLICKHOUSE_READY_TIMEOUT_SEC" >&2
  return 1
}

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    printf 'Required catalog file is missing: %s\n' "$path" >&2
    return 1
  fi
}

wait_for_clickhouse

require_file "$CATALOG_DIR/workstation_users.tsv"
require_file "$CATALOG_DIR/application_categories.tsv"
require_file "$CATALOG_DIR/domain_categories.tsv"

printf '[catalog] truncate dimension tables\n'
run_query "TRUNCATE TABLE $CLICKHOUSE_DATABASE.dim_workstation_user"
run_query "TRUNCATE TABLE $CLICKHOUSE_DATABASE.dim_application_category"
run_query "TRUNCATE TABLE $CLICKHOUSE_DATABASE.dim_domain_category"

printf '[catalog] load workstation users\n'
run_insert_file "
INSERT INTO $CLICKHOUSE_DATABASE.dim_workstation_user
    (host_name, user_login, user_domain, employee_id, employee_name, department, branch, position, source, is_active)
FORMAT TabSeparatedWithNames
" "$CATALOG_DIR/workstation_users.tsv"

printf '[catalog] load application categories\n'
run_insert_file "
INSERT INTO $CLICKHOUSE_DATABASE.dim_application_category
    (process_name, application_name, vendor, category, productivity_class, risk_level, is_system, source, comment, is_active)
FORMAT TabSeparatedWithNames
" "$CATALOG_DIR/application_categories.tsv"

printf '[catalog] load domain categories\n'
run_insert_file "
INSERT INTO $CLICKHOUSE_DATABASE.dim_domain_category
    (domain, site_name, category, productivity_class, risk_level, business_allowed, source, comment, is_active)
FORMAT TabSeparatedWithNames
" "$CATALOG_DIR/domain_categories.tsv"

printf '[catalog] reload dictionaries\n'
run_query "SYSTEM RELOAD DICTIONARY $CLICKHOUSE_DATABASE.dict_workstation_user"
run_query "SYSTEM RELOAD DICTIONARY $CLICKHOUSE_DATABASE.dict_application_category"
run_query "SYSTEM RELOAD DICTIONARY $CLICKHOUSE_DATABASE.dict_domain_category"

if [[ "$REBUILD_AGGREGATES" == "1" ]]; then
  printf '[catalog] rebuild aggregates\n'
  run_query_file "$ROOT_DIR/admin/rebuild_aggregates.sql"
fi

printf '[catalog] dictionary status\n'
run_query "
SELECT name, status, last_exception
FROM system.dictionaries
WHERE database = '$CLICKHOUSE_DATABASE'
  AND name IN ('dict_workstation_user', 'dict_application_category', 'dict_domain_category')
ORDER BY name
FORMAT PrettyCompact
"

printf '[catalog] raw unknown summary\n'
run_query "
SELECT 'subjects' AS area, count() AS rows
FROM $CLICKHOUSE_DATABASE.v_workforce_unknown_subjects
UNION ALL
SELECT 'processes' AS area, count() AS rows
FROM $CLICKHOUSE_DATABASE.v_workforce_unknown_processes
UNION ALL
SELECT 'domains' AS area, count() AS rows
FROM $CLICKHOUSE_DATABASE.v_workforce_unknown_domains
FORMAT PrettyCompact
"
