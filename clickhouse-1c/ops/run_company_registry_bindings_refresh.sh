#!/usr/bin/env bash
set -euo pipefail

ROOT="${AW_1C_ROOT:-/opt/activitywatch/clickhouse-1c}"
ENV_FILE="${ROOT}/.env"
VENV="${ROOT}/.venv"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing env file: ${ENV_FILE}" >&2
  exit 1
fi

if [[ ! -x "${VENV}/bin/python" ]]; then
  echo "missing venv python: ${VENV}/bin/python" >&2
  exit 1
fi

set -a
# shellcheck source=/dev/null
. "${ENV_FILE}"
set +a

CH_RUNTIME_HOST="${AW_1C_CLICKHOUSE_RUNTIME_HOST:-${CLICKHOUSE_HOST}}"
if [[ "${CH_RUNTIME_HOST}" == "clickhouse" ]]; then
  CH_RUNTIME_HOST="127.0.0.1"
fi
: "${CLICKHOUSE_PORT:?CLICKHOUSE_PORT is required}"

"${VENV}/bin/python" "${ROOT}/ai/refresh_company_registry_bindings.py" \
  --host "${CH_RUNTIME_HOST}" \
  --port "${CLICKHOUSE_PORT}" \
  --user "${CLICKHOUSE_USER}" \
  --database "${CLICKHOUSE_DB}"
