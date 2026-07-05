#!/usr/bin/env bash
set -euo pipefail

# Canonical daily/weekly AWatch-rus contour check launcher.
# Read-only: does not push, tag, heal, restart services, or change Git history.
# Live endpoints must be supplied through environment files outside the public
# repository.

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${AWATCH_REPO_ROOT:-$(cd -- "${SCRIPT_DIR}/.." && pwd)}"
SCOPE="${CONTOUR_CHECK_SCOPE:-daily}"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
OUTPUT_ROOT="${CONTOUR_CHECK_OUTPUT_ROOT:-${REPO_ROOT}/.ops/contour-check-runs}"
OUTPUT_DIR="${OUTPUT_DIR:-${OUTPUT_ROOT}/${SCOPE}-${RUN_ID}}"
ENV_FILE="${CONTOUR_CHECK_ENV_FILE:-${REPO_ROOT}/private-config/awatch-contour-check.env}"
LIB_DIR="${CONTOUR_CHECK_LIB_DIR:-/usr/local/lib/awatch-contour}"
SMOKE_TIMEOUT_SECONDS="${CONTOUR_CHECK_SMOKE_TIMEOUT_SECONDS:-120}"

mkdir -p "${OUTPUT_DIR}/logs"

if [[ -f "${ENV_FILE}" ]]; then
  # shellcheck disable=SC1090
  source "${ENV_FILE}"
fi

configure_detmir_env() {
  if [[ -z "${DETMIR_DLP_COMMAND:-}" ]]; then
    export DETMIR_DLP_COMMAND="detmir-dlp"
  fi

  if [[ -z "${DETMIR_PORTAL_URL:-}" ]]; then
    export DETMIR_PORTAL_URL="http://127.0.0.1:8720"
  fi

  if [[ -z "${DETMIR_GATEWAY_HOST:-}" ]]; then
    local derived_host="${DETMIR_PORTAL_URL#*://}"
    derived_host="${derived_host%%/*}"
    derived_host="${derived_host%%:*}"
    if [[ -n "${derived_host}" ]]; then
      export DETMIR_GATEWAY_HOST="${derived_host}"
    else
      export DETMIR_GATEWAY_HOST="127.0.0.1"
    fi
  fi

  export DETMIR_DLP_ENABLED="${DETMIR_DLP_ENABLED:-${AW_DLP_ENABLED:-false}}"
  require_live_value DETMIR_AW_API
  require_live_value DETMIR_WORKTIME_URL
  require_live_value DETMIR_ONE_C_URL
  require_live_value DETMIR_RDP_HOST
  require_live_value DETMIR_HOSTNAME
}

require_live_value() {
  local name="$1"
  local value="${!name:-}"
  if [[ -z "${value}" ]]; then
    printf 'Missing required live contour variable: %s. Set it in %s or the environment.\n' "${name}" "${ENV_FILE}" >&2
    exit 2
  fi
  case "${value}" in
    *192.0.2.*|*198.51.100.*|*203.0.113.*|*HOST-EXAMPLE*|*.example*)
      printf 'Refusing placeholder value for %s: %s\n' "${name}" "${value}" >&2
      exit 2
      ;;
  esac
}

normalize_http_base() {
  local value="${1:-}"
  value="${value%/}"
  case "${value}" in
    "") return 1 ;;
    http://*|https://*) printf '%s' "${value}" ;;
    *) printf 'http://%s' "${value}" ;;
  esac
}

normalize_url_env() {
  local name="$1"
  local value="${!name:-}"
  if [[ -n "${value}" ]]; then
    export "${name}=$(normalize_http_base "${value}")"
  fi
}

portal_smoke_url_from_portal_url() {
  local value="${1%/}"
  case "${value}" in
    */portal) printf '%s/\n' "${value}" ;;
    *) printf '%s/portal/\n' "${value}" ;;
  esac
}

configure_smoke_env() {
  normalize_url_env DETMIR_AW_API
  normalize_url_env DETMIR_WORKTIME_URL
  normalize_url_env DETMIR_ONE_C_URL
  normalize_url_env DETMIR_PORTAL_URL

  export \
    DETMIR_AW_API \
    DETMIR_WORKTIME_URL \
    DETMIR_ONE_C_URL \
    DETMIR_RDP_HOST \
    DETMIR_HOSTNAME \
    DETMIR_GATEWAY_HOST \
    DETMIR_PORTAL_URL \
    DETMIR_DLP_ENABLED \
    DETMIR_DISABLE_PORTAL_CHECK \
    DETMIR_DISABLE_DLP_HEALTH_CHECK \
    DETMIR_PCT_BIN \
    DETMIR_GRAFANA_CHECK_JSON \
    DETMIR_PORTAL_SMOKE_BASIC_AUTH \
    DETMIR_BASIC_AUTH \
    DETMIR_PORTAL_SMOKE_AUTH_HEADER \
    DETMIR_PORTAL_AUTH_HEADER

  case "${AWATCH_PORTAL_SMOKE_URL:-}" in
    ""|http://127.0.0.1:8720*)
      export AWATCH_PORTAL_SMOKE_URL="${DETMIR_PORTAL_URL}"
      ;;
  esac
  case "${DETMIR_PORTAL_SMOKE_URL:-}" in
    ""|http://127.0.0.1:8720*)
      DETMIR_PORTAL_SMOKE_URL="$(portal_smoke_url_from_portal_url "${DETMIR_PORTAL_URL}")"
      ;;
  esac
  export AWATCH_PORTAL_SMOKE_URL DETMIR_PORTAL_SMOKE_URL
  if [[ "${DETMIR_PORTAL_URL}" == https://* && -z "${DETMIR_PORTAL_SMOKE_INSECURE_TLS:-}" ]]; then
    export DETMIR_PORTAL_SMOKE_INSECURE_TLS=1
  fi
  export DETMIR_PORTAL_SMOKE_INSECURE_TLS
  if [[ -z "${DETMIR_PORTAL_AUTH_HEADER:-}" && -n "${DETMIR_PORTAL_SMOKE_BASIC_AUTH:-}" ]]; then
    export DETMIR_PORTAL_AUTH_HEADER="Basic ${DETMIR_PORTAL_SMOKE_BASIC_AUTH}"
  fi
  case "${DETMIR_DLP_ENABLED,,}" in
    0|false|no|off)
      export DETMIR_DISABLE_DLP_HEALTH_CHECK="${DETMIR_DISABLE_DLP_HEALTH_CHECK:-1}"
      ;;
  esac
}

write_summary() {
  {
    printf '# AWatch-rus contour check\n\n'
    printf -- '- scope: %s\n' "${SCOPE}"
    printf -- '- generated_at_utc: %s\n' "${RUN_ID}"
    printf -- '- output_dir: %s\n\n' "${OUTPUT_DIR}"
    printf 'GitHub/public CI is not the primary registry release contour. This run is intended for the Russian/internal operational contour.\n\n'
    printf 'DETMIR_AW_API=%s\n' "${DETMIR_AW_API}"
    printf 'DETMIR_WORKTIME_URL=%s\n' "${DETMIR_WORKTIME_URL}"
    printf 'DETMIR_ONE_C_URL=%s\n' "${DETMIR_ONE_C_URL}"
    printf 'DETMIR_RDP_HOST=%s\n' "${DETMIR_RDP_HOST}"
    printf 'DETMIR_HOSTNAME=%s\n' "${DETMIR_HOSTNAME}"
    printf 'DETMIR_GATEWAY_HOST=%s\n' "${DETMIR_GATEWAY_HOST}"
    printf 'DETMIR_PORTAL_URL=%s\n' "${DETMIR_PORTAL_URL}"
    printf 'DETMIR_DLP_ENABLED=%s\n' "${DETMIR_DLP_ENABLED}"
    printf 'DETMIR_DLP_COMMAND=%s\n' "${DETMIR_DLP_COMMAND}"
    printf 'DETMIR_DISABLE_PORTAL_CHECK=%s\n' "${DETMIR_DISABLE_PORTAL_CHECK:-0}"
    printf 'DETMIR_DISABLE_DLP_HEALTH_CHECK=%s\n' "${DETMIR_DISABLE_DLP_HEALTH_CHECK:-0}"
    printf '\n'
  } >"${OUTPUT_DIR}/SUMMARY.md"
}

configure_detmir_env
configure_smoke_env

find_detmir_check() {
  if [[ -n "${DETMIR_CHECK_BIN:-}" ]]; then
    printf '%s\n' "${DETMIR_CHECK_BIN}"
  elif command -v detmir-check >/dev/null 2>&1; then
    printf '%s\n' "detmir-check"
  elif [[ -x "${REPO_ROOT}/adk-rust/target/release/detmir-check" ]]; then
    printf '%s\n' "${REPO_ROOT}/adk-rust/target/release/detmir-check"
  else
    printf '%s\n' "cargo run --manifest-path ${REPO_ROOT}/adk-rust/Cargo.toml -p detmir-check --"
  fi
}

run_and_log() {
  local name="$1"
  shift
  local log="${OUTPUT_DIR}/logs/${name}.log"
  printf '== %s ==\n' "${name}" | tee -a "${OUTPUT_DIR}/SUMMARY.md"
  local started_at
  started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'started_at_utc: %s\n' "${started_at}" | tee -a "${OUTPUT_DIR}/SUMMARY.md"
  local rc=0
  local started_at_unix
  started_at_unix="$(date -u +%s)"

  local stream_logs="${CONTOUR_CHECK_STREAM:-0}"
  if [[ "${stream_logs}" == "1" && -t 1 ]]; then
    "$@" >"${log}" 2>&1 &
    local _run_pid=$!
    while kill -0 "${_run_pid}" 2>/dev/null; do
      sleep 10
      local now
      now="$(date -u +%s)"
      local running_elapsed=$(( now - started_at_unix ))
      printf 'running %s for %ss\n' "${name}" "${running_elapsed}"
    done
    wait "${_run_pid}"; rc=$?
  else
    "$@" >"${log}" 2>&1 || rc=$?
  fi

  local finished_at elapsed
  finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  elapsed="$(( $(date -u -d "${finished_at}" +%s) - $(date -u -d "${started_at}" +%s) ))"
  if [[ "${rc}" -eq 0 ]]; then
    printf 'status: ok elapsed=%ss\n\n' "${elapsed}" | tee -a "${OUTPUT_DIR}/SUMMARY.md"
    return 0
  fi
  printf 'status: fail rc=%s elapsed=%ss log=%s\n\n' "${rc}" "${elapsed}" "${log}" | tee -a "${OUTPUT_DIR}/SUMMARY.md"
  return "${rc}"
}

run_node_smoke() {
  local name="$1"
  local script="$2"
  if command -v timeout >/dev/null 2>&1; then
    run_and_log "${name}" timeout "${SMOKE_TIMEOUT_SECONDS}" node "${script}"
  else
    run_and_log "${name}" node "${script}"
  fi
}

write_summary

status=0
detmir_check_cmd="$(find_detmir_check)"

if [[ "${detmir_check_cmd}" == cargo\ run* ]]; then
  # shellcheck disable=SC2086
  if ! run_and_log "detmir-check-json" bash -lc "${detmir_check_cmd} --json"; then
    status=1
  fi
else
  if ! run_and_log "detmir-check-json" "${detmir_check_cmd}" --json; then
    status=1
  fi
fi

if [[ "${RUN_PORTAL_SMOKE:-0}" == "1" ]]; then
  hardening_smoke="${REPO_ROOT}/scripts/awatch-production-hardening-smoke.mjs"
  if [[ ! -f "${hardening_smoke}" && -f "${LIB_DIR}/awatch-production-hardening-smoke.mjs" ]]; then
    hardening_smoke="${LIB_DIR}/awatch-production-hardening-smoke.mjs"
  fi
  if [[ -f "${hardening_smoke}" ]] && command -v node >/dev/null 2>&1; then
    if ! run_node_smoke "portal-hardening-smoke" "${hardening_smoke}"; then
      status=1
    fi
  else
    printf '== portal-hardening-smoke ==\nstatus: skipped reason=node_or_script_missing\n\n' | tee -a "${OUTPUT_DIR}/SUMMARY.md"
  fi

  pilot_smoke="${REPO_ROOT}/scripts/detmir-pilot-demo-smoke.mjs"
  if [[ -f "${pilot_smoke}" ]] && command -v node >/dev/null 2>&1; then
    if ! run_node_smoke "pilot-demo-smoke" "${pilot_smoke}"; then
      status=1
    fi
  else
    printf '== pilot-demo-smoke ==\nstatus: skipped reason=requires_repo_docs_and_fixtures\n\n' | tee -a "${OUTPUT_DIR}/SUMMARY.md"
  fi
else
  printf '== portal-smoke ==\nstatus: skipped reason=RUN_PORTAL_SMOKE_not_enabled\n\n' | tee -a "${OUTPUT_DIR}/SUMMARY.md"
fi

if [[ "${RUN_REGISTRY_CHECK:-0}" == "1" ]] && [[ -x "${REPO_ROOT}/scripts/registry_readiness_check.sh" ]]; then
  if ! run_and_log "registry-readiness-check" bash "${REPO_ROOT}/scripts/registry_readiness_check.sh"; then
    status=1
  fi
fi

if [[ "${RUN_RESILIENCE_CHECK:-0}" == "1" ]] && [[ -f "${REPO_ROOT}/scripts/detmir_resilience_check.sh" ]]; then
  resilience_mode="${RESILIENCE_CHECK_MODE:-repo}"
  if ! run_and_log "detmir-resilience-check" bash "${REPO_ROOT}/scripts/detmir_resilience_check.sh" "--${resilience_mode}"; then
    status=1
  fi
fi

printf 'final_status: %s\n' "$([[ "${status}" -eq 0 ]] && printf ok || printf fail)" | tee -a "${OUTPUT_DIR}/SUMMARY.md"
exit "${status}"
