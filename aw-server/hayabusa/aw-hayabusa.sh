#!/usr/bin/env bash
set -euo pipefail

HAYA_ROOT="${AW_HAYABUSA_ROOT:-/opt/hayabusa}"
HAYA_CURRENT="${HAYA_ROOT}/current"
HAYA_BIN="${HAYA_CURRENT}/hayabusa"
HAYA_RULES="${HAYA_CURRENT}/rules"
HAYA_RULES_CONFIG="${HAYA_RULES}/config"
HAYA_CONFIG="${HAYA_CURRENT}/config"
HAYA_REPORTS_ROOT="${AW_HAYABUSA_REPORTS_ROOT:-${HAYA_ROOT}/reports}"
HAYA_STATE_ROOT="${AW_HAYABUSA_STATE_ROOT:-${HAYA_ROOT}/state}"
HAYA_INCOMING_DIR="${AW_HAYABUSA_INCOMING_DIR:-${HAYA_ROOT}/inbox/incoming}"
HAYA_STAGING_DIR="${AW_HAYABUSA_STAGING_DIR:-${HAYA_ROOT}/inbox/staging}"
HAYA_ARCHIVE_PACKAGES_DIR="${AW_HAYABUSA_ARCHIVE_PACKAGES_DIR:-${HAYA_ROOT}/archive/packages}"
HAYA_ARCHIVE_EXTRACTED_DIR="${AW_HAYABUSA_ARCHIVE_EXTRACTED_DIR:-${HAYA_ROOT}/archive/extracted}"
HAYA_LOGS_DIR="${AW_HAYABUSA_LOGS_DIR:-${HAYA_ROOT}/state/logs}"
LAST_REPORT_DIR=""

usage() {
  cat <<'EOF'
Usage:
  aw-hayabusa doctor
  aw-hayabusa inventory
  aw-hayabusa accept --package <zip> [--host HOST]
  aw-hayabusa process-inbox [--mode <quick|incident|full>] [--limit N]
  aw-hayabusa profiles
  aw-hayabusa version
  aw-hayabusa <quick|incident|full> --input <file-or-dir> [--host HOST] [--label LABEL] [--output-root DIR] [--threads N]

Modes:
  quick     Fast CSV triage with HTML summary and logon summary
  incident  Rich JSONL timeline for incident review with HTML summary and logon summary
  full      Broad JSONL timeline with all rule families enabled, HTML summary and logon summary
EOF
}

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

sanitize() {
  printf '%s' "$1" | tr ' /:@' '_' | tr -cd 'A-Za-z0-9._-'
}

ensure_layout() {
  [ -x "${HAYA_BIN}" ] || fail "Hayabusa binary not found at ${HAYA_BIN}"
  [ -d "${HAYA_RULES}" ] || fail "Hayabusa rules directory not found at ${HAYA_RULES}"
  [ -d "${HAYA_CONFIG}" ] || fail "Hayabusa config directory not found at ${HAYA_CONFIG}"
  [ -d "${HAYA_RULES_CONFIG}" ] || fail "Hayabusa rules config directory not found at ${HAYA_RULES_CONFIG}"
  mkdir -p \
    "${HAYA_REPORTS_ROOT}" \
    "${HAYA_STATE_ROOT}" \
    "${HAYA_LOGS_DIR}" \
    "${HAYA_ROOT}/inbox" \
    "${HAYA_ROOT}/archive" \
    "${HAYA_INCOMING_DIR}" \
    "${HAYA_STAGING_DIR}" \
    "${HAYA_ARCHIVE_PACKAGES_DIR}" \
    "${HAYA_ARCHIVE_EXTRACTED_DIR}"
}

run_logged() {
  local log_file="$1"
  shift
  {
    printf '[%s] CMD:' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf ' %q' "$@"
    printf '\n'
  } | tee -a "${log_file}"
  "$@" 2>&1 | tee -a "${log_file}"
  return "${PIPESTATUS[0]}"
}

write_manifest() {
  local manifest_path="$1"
  local mode="$2"
  local host="$3"
  local input_path="$4"
  local report_dir="$5"
  local status="$6"
  local output_format="$7"
  cat >"${manifest_path}" <<EOF
{
  "mode": "${mode}",
  "host": "${host}",
  "input": "${input_path}",
  "report_dir": "${report_dir}",
  "status": "${status}",
  "output_format": "${output_format}",
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
}

json_field() {
  local json_path="$1"
  local field_name="$2"
  python3 - "$json_path" "$field_name" <<'PY'
import json, sys
path, field = sys.argv[1], sys.argv[2]
try:
    with open(path, 'r', encoding='utf-8') as fh:
        obj = json.load(fh)
except Exception:
    sys.exit(0)
value = obj.get(field)
if value is None:
    sys.exit(0)
print(str(value))
PY
}

detect_host_from_manifest() {
  local manifest_path="$1"
  local host=""
  host="$(json_field "${manifest_path}" host || true)"
  if [ -z "${host}" ]; then
    host="$(json_field "${manifest_path}" hostname || true)"
  fi
  printf '%s' "${host}"
}

extract_zip_normalized() {
  local package_path="$1"
  local dest_dir="$2"
  python3 - "${package_path}" "${dest_dir}" <<'PY'
import pathlib
import shutil
import sys
import zipfile

zip_path = pathlib.Path(sys.argv[1])
dest_dir = pathlib.Path(sys.argv[2])
dest_dir.mkdir(parents=True, exist_ok=True)

with zipfile.ZipFile(zip_path) as zf:
    for info in zf.infolist():
        raw_name = info.filename.replace('\\', '/')
        normalized = pathlib.PurePosixPath(raw_name)
        parts = [part for part in normalized.parts if part not in ('', '.')]
        if any(part == '..' for part in parts):
            raise SystemExit(f'unsafe zip entry: {info.filename}')
        if not parts:
            continue
        target = dest_dir.joinpath(*parts)
        is_dir = info.is_dir() or raw_name.endswith('/')
        if is_dir:
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        with zf.open(info) as src, target.open('wb') as dst:
            shutil.copyfileobj(src, dst)
PY
}

write_package_manifest() {
  local manifest_path="$1"
  local package_path="$2"
  local host="$3"
  local intake_id="$4"
  local sha256="$5"
  local status="$6"
  local stage_dir="$7"
  local report_dir="$8"
  cat >"${manifest_path}" <<EOF
{
  "package_path": "${package_path}",
  "host": "${host}",
  "intake_id": "${intake_id}",
  "sha256": "${sha256}",
  "status": "${status}",
  "stage_dir": "${stage_dir}",
  "report_dir": "${report_dir}",
  "processed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
}

write_state_json() {
  local state_path="$1"
  local body="$2"
  printf '%s\n' "${body}" > "${state_path}"
}

run_mode() {
  local mode="$1"
  shift

  local input_path=""
  local host=""
  local label=""
  local output_root="${HAYA_REPORTS_ROOT}"
  local threads=""

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --input|-i)
        [ "$#" -ge 2 ] || fail "--input requires a value"
        input_path="$2"
        shift 2
        ;;
      --host)
        [ "$#" -ge 2 ] || fail "--host requires a value"
        host="$2"
        shift 2
        ;;
      --label)
        [ "$#" -ge 2 ] || fail "--label requires a value"
        label="$2"
        shift 2
        ;;
      --output-root)
        [ "$#" -ge 2 ] || fail "--output-root requires a value"
        output_root="$2"
        shift 2
        ;;
      --threads)
        [ "$#" -ge 2 ] || fail "--threads requires a value"
        threads="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        fail "Unknown argument: $1"
        ;;
    esac
  done

  [ -n "${input_path}" ] || fail "--input is required"
  [ -e "${input_path}" ] || fail "Input path does not exist: ${input_path}"

  ensure_layout
  mkdir -p "${output_root}"

  if [ -z "${host}" ]; then
    host="$(basename "${input_path}")"
    if [ "${host}" = "." ] || [ "${host}" = "/" ]; then
      host="unknown"
    fi
  fi
  host="$(sanitize "${host}")"
  [ -n "${host}" ] || host="unknown"

  local label_suffix=""
  if [ -n "${label}" ]; then
    label_suffix="_$(sanitize "${label}")"
  fi
  local run_ts
  run_ts="$(date -u +%Y%m%dT%H%M%SZ)"
  local report_dir="${output_root}/${host}/${run_ts}_${mode}${label_suffix}"
  local log_file="${report_dir}/run.log"
  local manifest_file="${report_dir}/manifest.json"
  local html_file="${report_dir}/summary.html"
  local timeline_file=""
  local output_format=""
  local -a input_args=()
  local -a common_args=("-w" "-q" "-C" "-r" "${HAYA_RULES}" "-O")
  local -a mode_args=()
  local -a command=()
  local -a logon_command=()

  mkdir -p "${report_dir}"

  if [ -d "${input_path}" ]; then
    input_args=("-d" "${input_path}")
  else
    input_args=("-f" "${input_path}")
  fi
  if [ -n "${threads}" ]; then
    common_args+=("-t" "${threads}")
  fi

  case "${mode}" in
    quick)
      timeline_file="${report_dir}/timeline.csv"
      output_format="csv"
      mode_args=("-E" "-P" "-m" "medium" "-o" "${timeline_file}" "-H" "${html_file}")
      command=("${HAYA_BIN}" "csv-timeline" "${input_args[@]}" "${common_args[@]}" "-c" "${HAYA_RULES_CONFIG}" "${mode_args[@]}")
      ;;
    incident)
      timeline_file="${report_dir}/timeline.jsonl"
      output_format="jsonl"
      mode_args=("-L" "-m" "low" "-o" "${timeline_file}" "-H" "${html_file}")
      command=("${HAYA_BIN}" "json-timeline" "${input_args[@]}" "${common_args[@]}" "-c" "${HAYA_RULES_CONFIG}" "${mode_args[@]}")
      ;;
    full)
      timeline_file="${report_dir}/timeline.jsonl"
      output_format="jsonl"
      mode_args=("-L" "-A" "-D" "-n" "-u" "-m" "informational" "-o" "${timeline_file}" "-H" "${html_file}")
      command=("${HAYA_BIN}" "json-timeline" "${input_args[@]}" "${common_args[@]}" "-c" "${HAYA_RULES_CONFIG}" "${mode_args[@]}")
      ;;
    *)
      fail "Unsupported mode: ${mode}"
      ;;
  esac

  logon_command=("${HAYA_BIN}" "logon-summary" "${input_args[@]}" "-q" "-C" "-c" "${HAYA_CONFIG}" "-O" "-o" "${report_dir}/logon-summary")

  {
    echo "mode=${mode}"
    echo "host=${host}"
    echo "input=${input_path}"
    echo "report_dir=${report_dir}"
    echo "output_format=${output_format}"
  } | tee -a "${log_file}" >/dev/null

  local status="ok"
  if ! run_logged "${log_file}" "${command[@]}"; then
    status="failed"
  fi
  if ! run_logged "${log_file}" "${logon_command[@]}"; then
    status="failed"
  fi

  write_manifest "${manifest_file}" "${mode}" "${host}" "${input_path}" "${report_dir}" "${status}" "${output_format}"
  ln -sfn "${report_dir}" "${HAYA_STATE_ROOT}/latest-run"
  ln -sfn "${report_dir}" "${HAYA_STATE_ROOT}/latest-${host}"
  LAST_REPORT_DIR="${report_dir}"

  echo "Report directory: ${report_dir}"
  if [ "${status}" != "ok" ]; then
    echo "ERROR: Hayabusa run failed; see ${log_file}" >&2
    return 1
  fi
}

inventory() {
  ensure_layout
  local incoming_count staged_count archived_pkg_count archived_extract_count
  incoming_count=$(find "${HAYA_INCOMING_DIR}" -maxdepth 1 -type f -name '*.zip' | wc -l)
  staged_count=$(find "${HAYA_STAGING_DIR}" -mindepth 1 -maxdepth 1 -type d | wc -l)
  archived_pkg_count=$(find "${HAYA_ARCHIVE_PACKAGES_DIR}" -type f -name '*.zip' | wc -l)
  archived_extract_count=$(find "${HAYA_ARCHIVE_EXTRACTED_DIR}" -mindepth 2 -maxdepth 2 -type d | wc -l)
  echo "aw-hayabusa inventory"
  echo "incoming_zip=${incoming_count}"
  echo "staged_dirs=${staged_count}"
  echo "archived_packages=${archived_pkg_count}"
  echo "archived_payloads=${archived_extract_count}"
  if [ -L "${HAYA_STATE_ROOT}/latest-run" ]; then
    echo "latest_run=$(readlink -f "${HAYA_STATE_ROOT}/latest-run")"
  fi
}

accept_package() {
  local package_path=""
  local host=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --package)
        [ "$#" -ge 2 ] || fail "--package requires a value"
        package_path="$2"
        shift 2
        ;;
      --host)
        [ "$#" -ge 2 ] || fail "--host requires a value"
        host="$2"
        shift 2
        ;;
      *)
        fail "Unknown argument: $1"
        ;;
    esac
  done
  [ -n "${package_path}" ] || fail "--package is required"
  [ -f "${package_path}" ] || fail "Package not found: ${package_path}"
  ensure_layout

  local ts base_name safe_base dest_path sha256
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  base_name="$(basename "${package_path}")"
  safe_base="$(sanitize "${base_name}")"
  [ -n "${safe_base}" ] || safe_base="incoming.zip"
  dest_path="${HAYA_INCOMING_DIR}/${ts}_${safe_base}"
  cp -f "${package_path}" "${dest_path}"
  sha256="$(sha256sum "${dest_path}" | awk '{print $1}')"
  printf '%s  %s\n' "${sha256}" "$(basename "${dest_path}")" > "${dest_path}.sha256"
  if [ -n "${host}" ]; then
    write_state_json "${dest_path}.host" "${host}"
  fi
  echo "Accepted package: ${dest_path}"
}

find_manifest_path() {
  local stage_dir="$1"
  find "${stage_dir}" -type f -name 'manifest.json' | head -n 1
}

find_evtx_root() {
  local stage_dir="$1"
  if [ -d "${stage_dir}/evtx" ]; then
    printf '%s' "${stage_dir}/evtx"
    return 0
  fi
  find "${stage_dir}" -type d -name evtx | head -n 1
}

process_one_package() {
  local package_path="$1"
  local mode="$2"
  local forced_host="${3:-}"

  ensure_layout

  local package_name package_base intake_id stage_dir package_sha256
  package_name="$(basename "${package_path}")"
  package_base="${package_name%.zip}"
  intake_id="$(sanitize "${package_base}")"
  stage_dir="${HAYA_STAGING_DIR}/${intake_id}"
  mkdir -p "${stage_dir}"

  package_sha256="$(sha256sum "${package_path}" | awk '{print $1}')"
  if ! extract_zip_normalized "${package_path}" "${stage_dir}"; then
    fail "normalized zip extraction failed for ${package_path}"
  fi

  local manifest_path host evtx_root archive_pkg_dir archive_pkg_path archive_extract_dir status report_dir
  manifest_path="$(find_manifest_path "${stage_dir}")"
  host="${forced_host}"
  if [ -z "${host}" ] && [ -f "${package_path}.host" ]; then
    host="$(cat "${package_path}.host" 2>/dev/null || true)"
  fi
  if [ -z "${host}" ] && [ -n "${manifest_path}" ]; then
    host="$(detect_host_from_manifest "${manifest_path}")"
  fi
  if [ -z "${host}" ]; then
    host="${package_base%%-*}"
  fi
  host="$(sanitize "${host}")"
  [ -n "${host}" ] || host="unknown"

  archive_pkg_dir="${HAYA_ARCHIVE_PACKAGES_DIR}/${host}"
  archive_extract_dir="${HAYA_ARCHIVE_EXTRACTED_DIR}/${host}/${intake_id}"
  mkdir -p "${archive_pkg_dir}" "${archive_extract_dir}"

  evtx_root="$(find_evtx_root "${stage_dir}")"
  status="ok"
  report_dir=""
  if [ -z "${evtx_root}" ] || ! find "${evtx_root}" -type f \( -iname '*.evtx' -o -iname '*.json' -o -iname '*.jsonl' \) | grep -q .; then
    status="failed-no-evtx"
  else
    if run_mode "${mode}" --input "${evtx_root}" --host "${host}" --label "${package_base}"; then
      report_dir="${LAST_REPORT_DIR}"
      status="ok"
    else
      report_dir="${LAST_REPORT_DIR}"
      status="failed-analysis"
    fi
  fi

  mv "${package_path}" "${archive_pkg_dir}/${intake_id}.zip"
  [ -f "${package_path}.sha256" ] && mv "${package_path}.sha256" "${archive_pkg_dir}/${intake_id}.zip.sha256"
  [ -f "${package_path}.host" ] && mv "${package_path}.host" "${archive_pkg_dir}/${intake_id}.host"
  mv "${stage_dir}" "${archive_extract_dir}/payload"
  write_package_manifest "${archive_extract_dir}/intake.json" "${archive_pkg_dir}/${intake_id}.zip" "${host}" "${intake_id}" "${package_sha256}" "${status}" "${archive_extract_dir}/payload" "${report_dir}"
  write_state_json "${HAYA_STATE_ROOT}/latest-intake.json" "$(cat "${archive_extract_dir}/intake.json")"
  echo "Processed package: ${archive_pkg_dir}/${intake_id}.zip"
  echo "Archive payload: ${archive_extract_dir}/payload"
  if [ -n "${report_dir}" ]; then
    echo "Report directory: ${report_dir}"
  fi
  [ "${status}" = "ok" ] || fail "Package workflow ended with status=${status}; archived for inspection"
}

process_inbox() {
  local mode="incident"
  local limit="0"
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --mode)
        [ "$#" -ge 2 ] || fail "--mode requires a value"
        mode="$2"
        shift 2
        ;;
      --limit)
        [ "$#" -ge 2 ] || fail "--limit requires a value"
        limit="$2"
        shift 2
        ;;
      *)
        fail "Unknown argument: $1"
        ;;
    esac
  done
  case "${mode}" in
    quick|incident|full) ;;
    *) fail "Unsupported mode for process-inbox: ${mode}" ;;
  esac
  ensure_layout

  local count=0 pkg
  while IFS= read -r pkg; do
    process_one_package "${pkg}" "${mode}"
    count=$((count + 1))
    if [ "${limit}" -gt 0 ] && [ "${count}" -ge "${limit}" ]; then
      break
    fi
  done < <(find "${HAYA_INCOMING_DIR}" -maxdepth 1 -type f -name '*.zip' | sort)
  [ "${count}" -gt 0 ] || echo "No packages in ${HAYA_INCOMING_DIR}"
}

main() {
  local subcommand="${1:-}"
  case "${subcommand}" in
    doctor)
      ensure_layout
      echo "aw-hayabusa doctor: OK"
      echo "root=${HAYA_ROOT}"
      echo "current=${HAYA_CURRENT}"
      echo "binary=${HAYA_BIN}"
      echo "rules=${HAYA_RULES}"
      echo "config=${HAYA_CONFIG}"
      echo "rules_config=${HAYA_RULES_CONFIG}"
      echo "reports=${HAYA_REPORTS_ROOT}"
      echo "state=${HAYA_STATE_ROOT}"
      echo "incoming=${HAYA_INCOMING_DIR}"
      echo "staging=${HAYA_STAGING_DIR}"
      echo "archive_packages=${HAYA_ARCHIVE_PACKAGES_DIR}"
      echo "archive_extracted=${HAYA_ARCHIVE_EXTRACTED_DIR}"
      echo "logs=${HAYA_LOGS_DIR}"
      ;;
    inventory)
      inventory
      ;;
    accept)
      shift
      accept_package "$@"
      ;;
    process-inbox)
      shift
      process_inbox "$@"
      ;;
    profiles)
      ensure_layout
      cd "${HAYA_CURRENT}"
      exec "${HAYA_BIN}" list-profiles
      ;;
    version)
      ensure_layout
      cd "${HAYA_CURRENT}"
      exec "${HAYA_BIN}" help
      ;;
    quick|incident|full)
      shift
      cd "${HAYA_CURRENT}"
      run_mode "${subcommand}" "$@"
      ;;
    ""|-h|--help|help)
      usage
      ;;
    *)
      fail "Unknown subcommand: ${subcommand}"
      ;;
  esac
}

main "$@"
