#!/usr/bin/env bash
set -euo pipefail

ROOT="${AW_1C_ROOT:-/opt/activitywatch/clickhouse-1c}"
ENV_FILE="${ROOT}/.env"
VENV="${ROOT}/.venv"
CONFIG="${ROOT}/etl/config.yml"
CH_CONTAINER="${AW_1C_CLICKHOUSE_CONTAINER:-aw-rus-1c-clickhouse}"
LOCK_FILE="${ROOT}/.ingest.lock"
RUN_MANAGER_BRIEF_AFTER_INGEST="${AW_1C_MANAGER_BRIEF_RUN_AFTER_INGEST:-1}"
RUN_RECOVERY_BRIEF_AFTER_INGEST="${AW_1C_RECOVERY_BRIEF_RUN_AFTER_INGEST:-1}"
RUN_PROOFCHECK_AFTER_INGEST="${AW_1C_PROOFCHECK_RUN_AFTER_INGEST:-1}"
RUN_MCP_TOOLKIT_EXTRACT_BEFORE_INGEST="${AW_1C_MCP_TOOLKIT_EXTRACT_BEFORE_INGEST:-0}"

if [[ ! -f "${ENV_FILE}" ]]; then
  echo "missing env file: ${ENV_FILE}" >&2
  exit 1
fi

if [[ ! -f "${CONFIG}" ]]; then
  echo "missing etl config: ${CONFIG}" >&2
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

exec 9>"${LOCK_FILE}"
if ! flock -n 9; then
  echo "ingest cycle already running" >&2
  exit 0
fi

# shellcheck disable=SC1090
. "${ENV_FILE}"

if [[ "${RUN_MCP_TOOLKIT_EXTRACT_BEFORE_INGEST}" == "1" ]]; then
  "${VENV}/bin/python" "${ROOT}/etl/extract_1c_mcp_toolkit.py" --config "${CONFIG}"
fi

"${VENV}/bin/python" "${ROOT}/etl/build_business_event_exports.py" --config "${CONFIG}"
"${VENV}/bin/python" "${ROOT}/etl/load_1c_exports.py" --config "${CONFIG}"
"${VENV}/bin/python" "${ROOT}/etl/load_company_registry_xlsx.py" --config "${CONFIG}" --landing "${ROOT}/landing/registry"

docker exec -i "${CH_CONTAINER}" clickhouse-client \
  --user "${CLICKHOUSE_USER}" \
  --password "${CLICKHOUSE_PASSWORD}" \
  --database "${CLICKHOUSE_DB}" \
  < "${ROOT}/detections/build_entity_timeline.sql"

docker exec -i "${CH_CONTAINER}" clickhouse-client \
  --user "${CLICKHOUSE_USER}" \
  --password "${CLICKHOUSE_PASSWORD}" \
  --database "${CLICKHOUSE_DB}" \
  < "${ROOT}/clickhouse/init/04_company_intelligence.sql"

"${ROOT}/ops/run_company_registry_bindings_refresh.sh"
"${ROOT}/ops/run_company_intelligence_refresh.sh"

docker exec -i "${CH_CONTAINER}" clickhouse-client \
  --user "${CLICKHOUSE_USER}" \
  --password "${CLICKHOUSE_PASSWORD}" \
  --database "${CLICKHOUSE_DB}" \
  < "${ROOT}/detections/insert_detections.sql"

docker exec -i "${CH_CONTAINER}" clickhouse-client \
  --user "${CLICKHOUSE_USER}" \
  --password "${CLICKHOUSE_PASSWORD}" \
  --database "${CLICKHOUSE_DB}" \
  < "${ROOT}/detections/open_cases_from_detections.sql"

if [[ "${RUN_MANAGER_BRIEF_AFTER_INGEST}" == "1" ]]; then
  if ! "${ROOT}/ops/run_manager_brief.sh"; then
    echo "warning: manager brief refresh failed after ingest" >&2
  fi
fi

if [[ "${RUN_RECOVERY_BRIEF_AFTER_INGEST}" == "1" ]]; then
  if ! "${ROOT}/ops/run_recovery_brief.sh"; then
    echo "warning: recovery brief refresh failed after ingest" >&2
  fi
fi

if [[ "${RUN_PROOFCHECK_AFTER_INGEST}" == "1" ]]; then
  if ! "${ROOT}/ops/check_ingest_freshness.sh"; then
    echo "warning: freshness proof check failed after ingest" >&2
  fi
fi
