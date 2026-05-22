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

# shellcheck disable=SC1090
set -a
. "${ENV_FILE}"
set +a

CH_RUNTIME_HOST="${AW_1C_CLICKHOUSE_RUNTIME_HOST:-${CLICKHOUSE_HOST}}"
if [[ "${CH_RUNTIME_HOST}" == "clickhouse" ]]; then
  CH_RUNTIME_HOST="127.0.0.1"
fi

export CLICKHOUSE_HOST="${CH_RUNTIME_HOST}"

exec "${VENV}/bin/python" "${ROOT}/ai/generate_manager_brief.py" \
  --host "${CH_RUNTIME_HOST}" \
  --port "${CLICKHOUSE_PORT}" \
  --user "${CLICKHOUSE_USER}" \
  --password "${CLICKHOUSE_PASSWORD}" \
  --database "${CLICKHOUSE_DB}"
