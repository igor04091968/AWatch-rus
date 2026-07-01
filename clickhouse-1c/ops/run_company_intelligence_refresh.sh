#!/usr/bin/env bash
# shellcheck disable=SC2119
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

set -a
# shellcheck source=/dev/null
. "${ENV_FILE}"
set +a
# shellcheck source=clickhouse-1c/ops/clickhouse-client-safe.sh
. "${ROOT}/ops/clickhouse-client-safe.sh"

CH_RUNTIME_HOST="${AW_1C_CLICKHOUSE_RUNTIME_HOST:-${CLICKHOUSE_HOST}}"
if [[ "${CH_RUNTIME_HOST}" == "clickhouse" ]]; then
  CH_RUNTIME_HOST="127.0.0.1"
fi
: "${CLICKHOUSE_PORT:?CLICKHOUSE_PORT is required}"

aw_1c_clickhouse_client \
  < "${ROOT}/clickhouse/init/04_company_intelligence.sql"

"${ROOT}/ops/run_company_registry_bindings_refresh.sh"

"${VENV}/bin/python" "${ROOT}/ai/refresh_company_intelligence.py" \
  --host "${CH_RUNTIME_HOST}" \
  --port "${CLICKHOUSE_PORT}" \
  --user "${CLICKHOUSE_USER}" \
  --database "${CLICKHOUSE_DB}"
