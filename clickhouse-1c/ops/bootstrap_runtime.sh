#!/usr/bin/env bash
set -euo pipefail

ROOT="${AW_1C_ROOT:-/opt/activitywatch/clickhouse-1c}"

mkdir -p \
  "${ROOT}/landing/documents" \
  "${ROOT}/landing/postings" \
  "${ROOT}/landing/companies" \
  "${ROOT}/landing/registry" \
  "${ROOT}/landing/reglog" \
  "${ROOT}/landing/audit" \
  "${ROOT}/landing/host" \
  "${ROOT}/archive/documents" \
  "${ROOT}/archive/postings" \
  "${ROOT}/archive/companies" \
  "${ROOT}/archive/registry" \
  "${ROOT}/archive/reglog" \
  "${ROOT}/archive/audit" \
  "${ROOT}/archive/host" \
  "${ROOT}/state/manager-brief/history"

if [[ ! -f "${ROOT}/etl/config.yml" ]]; then
  cp "${ROOT}/etl/config.example.yml" "${ROOT}/etl/config.yml"
fi

python3 -m venv "${ROOT}/.venv"
"${ROOT}/.venv/bin/pip" install --upgrade pip
"${ROOT}/.venv/bin/pip" install -r "${ROOT}/etl/requirements.txt"
"${ROOT}/.venv/bin/pip" install -r "${ROOT}/ai/requirements.txt"
