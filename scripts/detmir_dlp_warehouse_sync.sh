#!/usr/bin/env bash
set -euo pipefail

SOURCE_HOST="${AW_DLP_WAREHOUSE_SOURCE_HOST:-igor@10.10.10.13}"
SOURCE_PATH="${AW_DLP_WAREHOUSE_SOURCE_PATH:-/var/lib/activitywatch/dlp_warehouse.sqlite}"
DEST_PATH="${AW_DLP_WAREHOUSE_DEST_PATH:-/var/lib/activitywatch/dlp_warehouse.sqlite}"
STATE_DIR="${AW_DLP_WAREHOUSE_SYNC_STATE_DIR:-/var/lib/activitywatch/health}"
STATE_FILE="${AW_DLP_WAREHOUSE_SYNC_STATE_FILE:-${STATE_DIR}/dlp-warehouse-sync-state.json}"
SSH_OPTS="${AW_DLP_WAREHOUSE_SSH_OPTS:--o BatchMode=yes -o ConnectTimeout=5}"
REMOTE_TMP="/tmp/dlp_warehouse_sync_$$.sqlite"
LOCAL_TMP=""

json_string() {
  python3 -c 'import json,sys; print(json.dumps(sys.argv[1], ensure_ascii=False))' "$1"
}

write_state() {
  local status="$1"
  local message="$2"
  local rows="${3:-}"
  local bytes="${4:-}"
  local now tmp
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  mkdir -p "$STATE_DIR"
  tmp="$(mktemp "${STATE_FILE}.tmp.XXXXXX")"
  {
    printf '{'
    printf '"generated_at_utc":%s,' "$(json_string "$now")"
    printf '"status":%s,' "$(json_string "$status")"
    printf '"message":%s,' "$(json_string "$message")"
    printf '"source_host":%s,' "$(json_string "$SOURCE_HOST")"
    printf '"source_path":%s,' "$(json_string "$SOURCE_PATH")"
    printf '"dest_path":%s,' "$(json_string "$DEST_PATH")"
    if [[ "$rows" =~ ^[0-9]+$ ]]; then
      printf '"dlp_events":%s,' "$rows"
    else
      printf '"dlp_events":null,'
    fi
    if [[ "$bytes" =~ ^[0-9]+$ ]]; then
      printf '"bytes":%s' "$bytes"
    else
      printf '"bytes":null'
    fi
    printf '}\n'
  } >"$tmp"
  mv "$tmp" "$STATE_FILE"
}

cleanup_remote() {
  ssh $SSH_OPTS "$SOURCE_HOST" "rm -f '$REMOTE_TMP'" >/dev/null 2>&1 || true
}

main() {
  local dest_dir rows bytes
  dest_dir="$(dirname "$DEST_PATH")"
  mkdir -p "$dest_dir" "$STATE_DIR"
  LOCAL_TMP="$(mktemp "${DEST_PATH}.tmp.XXXXXX")"
  trap 'rm -f "${LOCAL_TMP:-}"; cleanup_remote' EXIT

  ssh $SSH_OPTS "$SOURCE_HOST" \
    "set -euo pipefail; if command -v sqlite3 >/dev/null 2>&1; then sqlite3 '$SOURCE_PATH' \".backup '$REMOTE_TMP'\" || cp -f '$SOURCE_PATH' '$REMOTE_TMP'; else cp -f '$SOURCE_PATH' '$REMOTE_TMP'; fi; test -s '$REMOTE_TMP'"
  scp $SSH_OPTS "$SOURCE_HOST:$REMOTE_TMP" "$LOCAL_TMP"
  chmod 0644 "$LOCAL_TMP"
  mv "$LOCAL_TMP" "$DEST_PATH"

  bytes="$(stat -c %s "$DEST_PATH" 2>/dev/null || printf '')"
  rows="$(sqlite3 "$DEST_PATH" 'select count(*) from dlp_events;' 2>/dev/null || printf '')"
  write_state "ok" "synced" "$rows" "$bytes"
  printf 'dlp warehouse synced: source=%s:%s dest=%s rows=%s bytes=%s\n' \
    "$SOURCE_HOST" "$SOURCE_PATH" "$DEST_PATH" "${rows:-unknown}" "${bytes:-unknown}"
}

main "$@"
