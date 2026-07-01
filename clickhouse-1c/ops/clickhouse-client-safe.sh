#!/usr/bin/env bash

aw_1c_xml_escape() {
  local value="$1"
  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"
  value="${value//\"/&quot;}"
  value="${value//\'/&apos;}"
  printf '%s' "${value}"
}

aw_1c_clickhouse_client() {
  local container="${CH_CONTAINER:-${AW_1C_CLICKHOUSE_CONTAINER:-aw-rus-1c-clickhouse}}"
  local local_cfg remote_cfg status

  if [[ -z "${CLICKHOUSE_USER:-}" ]]; then
    echo "CLICKHOUSE_USER is required" >&2
    return 1
  fi
  if [[ -z "${CLICKHOUSE_DB:-}" ]]; then
    echo "CLICKHOUSE_DB is required" >&2
    return 1
  fi
  if [[ -z "${CLICKHOUSE_PASSWORD+x}" ]]; then
    echo "CLICKHOUSE_PASSWORD is required" >&2
    return 1
  fi

  local_cfg="$(mktemp "${TMPDIR:-/tmp}/aw-1c-clickhouse-client.XXXXXX.xml")"
  chmod 0600 "${local_cfg}"
  remote_cfg="/tmp/aw-1c-clickhouse-client.$(date +%s).$$.xml"

  {
    printf '<config>\n'
    printf '  <user>%s</user>\n' "$(aw_1c_xml_escape "${CLICKHOUSE_USER}")"
    printf '  <password>%s</password>\n' "$(aw_1c_xml_escape "${CLICKHOUSE_PASSWORD}")"
    printf '  <database>%s</database>\n' "$(aw_1c_xml_escape "${CLICKHOUSE_DB}")"
    printf '</config>\n'
  } > "${local_cfg}"

  if ! docker exec -i "${container}" sh -c 'umask 077 && cat > "$1"' sh "${remote_cfg}" < "${local_cfg}"; then
    rm -f "${local_cfg}"
    echo "failed to stage ClickHouse client config in container" >&2
    return 1
  fi
  rm -f "${local_cfg}"

  status=0
  docker exec -i "${container}" clickhouse-client --config-file "${remote_cfg}" "$@" || status=$?
  docker exec "${container}" rm -f "${remote_cfg}" >/dev/null 2>&1 || true
  return "${status}"
}
