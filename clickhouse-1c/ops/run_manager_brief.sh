#!/usr/bin/env bash
set -euo pipefail

ROOT="${AW_1C_ROOT:-/opt/activitywatch/clickhouse-1c}"
ENV_FILE="${ROOT}/.env"
VENV="${ROOT}/.venv"
LOCK_FILE="${ROOT}/.manager-brief.lock"
RETRIES="${AW_1C_MANAGER_BRIEF_RETRIES:-3}"
RETRY_DELAY_SEC="${AW_1C_MANAGER_BRIEF_RETRY_DELAY_SEC:-20}"
LOCK_WAIT_SEC="${AW_1C_MANAGER_BRIEF_LOCK_WAIT_SEC:-1200}"

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

export CLICKHOUSE_HOST="${CH_RUNTIME_HOST}"

exec 9>"${LOCK_FILE}"
if ! flock -w "${LOCK_WAIT_SEC}" 9; then
  echo "manager brief lock wait exceeded: ${LOCK_WAIT_SEC}s" >&2
  exit 1
fi

attempt=1
while (( attempt <= RETRIES )); do
  if "${VENV}/bin/python" "${ROOT}/ai/generate_manager_brief.py" \
    --host "${CH_RUNTIME_HOST}" \
    --port "${CLICKHOUSE_PORT}" \
    --user "${CLICKHOUSE_USER}" \
    --database "${CLICKHOUSE_DB}"; then
    exit 0
  fi
  if (( attempt == RETRIES )); then
    break
  fi
  echo "manager brief attempt ${attempt}/${RETRIES} failed, retrying in ${RETRY_DELAY_SEC}s" >&2
  sleep "${RETRY_DELAY_SEC}"
  attempt=$((attempt + 1))
done

echo "manager brief failed after ${RETRIES} attempts" >&2
exit 1
