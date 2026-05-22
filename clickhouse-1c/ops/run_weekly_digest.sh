#!/usr/bin/env bash
set -euo pipefail

ROOT="${AW_1C_ROOT:-/opt/activitywatch/clickhouse-1c}"
ENV_FILE="${ROOT}/.env"
VENV="${ROOT}/.venv"
LOCK_FILE="${ROOT}/.weekly-digest.lock"
LOCK_WAIT_SEC="${AW_1C_WEEKLY_DIGEST_LOCK_WAIT_SEC:-1800}"

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

exec 9>"${LOCK_FILE}"
if ! flock -w "${LOCK_WAIT_SEC}" 9; then
  echo "weekly digest lock wait exceeded: ${LOCK_WAIT_SEC}s" >&2
  exit 1
fi

exec "${VENV}/bin/python" "${ROOT}/ai/generate_weekly_digest.py"
