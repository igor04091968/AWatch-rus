#!/usr/bin/env bash
set -euo pipefail

ROOT="${AW_1C_ROOT:-/opt/activitywatch/clickhouse-1c}"
ENV_FILE="${ROOT}/.env"
CH_CONTAINER="${AW_1C_CLICKHOUSE_CONTAINER:-aw-rus-1c-clickhouse}"
MAX_AGE_HOURS="${AW_1C_MAX_AGE_HOURS:-8}"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing env file: ${ENV_FILE}" >&2
  exit 1
fi

if ! docker ps --format '{{.Names}}' | grep -qx "${CH_CONTAINER}"; then
  echo "clickhouse container not running: ${CH_CONTAINER}" >&2
  exit 1
fi

# shellcheck disable=SC1090
. "${ENV_FILE}"

query_max_age() {
  local table="$1"
  docker exec "${CH_CONTAINER}" clickhouse-client \
    --user "${CLICKHOUSE_USER}" \
    --password "${CLICKHOUSE_PASSWORD}" \
    --database "${CLICKHOUSE_DB}" \
    -q "SELECT if(count()=0, -1, dateDiff('hour', max(ts), now())) FROM ${table}"
}

documents_age="$(query_max_age documents)"
reglog_age="$(query_max_age reglog_events)"
audit_age="$(query_max_age audit_events)"
host_age="$(query_max_age host_events)"

printf 'freshness documents=%sh reglog=%sh audit=%sh host=%sh threshold=%sh\n' \
  "${documents_age}" "${reglog_age}" "${audit_age}" "${host_age}" "${MAX_AGE_HOURS}"

for age in "${documents_age}" "${reglog_age}" "${audit_age}" "${host_age}"; do
  if [[ "${age}" == "-1" ]]; then
    echo "one or more datasets are empty" >&2
    exit 1
  fi
  if (( age > MAX_AGE_HOURS )); then
    echo "freshness threshold exceeded" >&2
    exit 1
  fi
done
