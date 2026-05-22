#!/usr/bin/env bash
set -euo pipefail

ROOT="${AW_1C_ROOT:-/opt/activitywatch/clickhouse-1c}"
ENV_FILE="${ROOT}/.env"
VENV="${ROOT}/.venv"
CH_CONTAINER="${AW_1C_CLICKHOUSE_CONTAINER:-aw-rus-1c-clickhouse}"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing env file: ${ENV_FILE}" >&2
  exit 1
fi

if [[ ! -x "${VENV}/bin/python" ]]; then
  echo "missing venv python: ${VENV}/bin/python" >&2
  exit 1
fi

if ! docker ps --format '{{.Names}}' | grep -qx "${CH_CONTAINER}"; then
  echo "clickhouse container not running: ${CH_CONTAINER}" >&2
  exit 1
fi

# shellcheck disable=SC1090
. "${ENV_FILE}"

CH_RUNTIME_HOST="${AW_1C_CLICKHOUSE_RUNTIME_HOST:-${CLICKHOUSE_HOST}}"
if [[ "${CH_RUNTIME_HOST}" == "clickhouse" ]]; then
  CH_RUNTIME_HOST="127.0.0.1"
fi

docker exec -i "${CH_CONTAINER}" clickhouse-client \
  --user "${CLICKHOUSE_USER}" \
  --password "${CLICKHOUSE_PASSWORD}" \
  --database "${CLICKHOUSE_DB}" \
  < "${ROOT}/clickhouse/init/04_company_intelligence.sql"

"${VENV}/bin/python" "${ROOT}/ai/refresh_company_intelligence.py" \
  --host "${CH_RUNTIME_HOST}" \
  --port "${CLICKHOUSE_PORT}" \
  --user "${CLICKHOUSE_USER}" \
  --password "${CLICKHOUSE_PASSWORD}" \
  --database "${CLICKHOUSE_DB}"
