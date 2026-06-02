#!/usr/bin/env bash
set -euo pipefail

AW_DATA_DIR="${AW_DATA_DIR:-/var/lib/activitywatch}"
BACKUP_DIR="${AW_BACKUP_DIR:-${AW_DATA_DIR}/backups}"
KEEP_DAYS="${AW_BACKUP_RETENTION_DAYS:-7}"
KEEP_LAST_DB="${AW_BACKUP_KEEP_LAST_DB:-2}"
KEEP_LAST_JSON="${AW_BACKUP_KEEP_LAST_JSON:-2}"

prune_group() {
  local keep_last="$1"
  local keep_days="$2"
  shift 2
  local files=()
  local idx=0
  local cutoff
  cutoff="$(date -d "-${keep_days} days" +%s)"
  mapfile -t files < <(find "$@" -maxdepth 1 -type f -printf '%T@ %p\n' 2>/dev/null | sort -nr | awk '{ $1=""; sub(/^ /,""); print }')
  for path in "${files[@]}"; do
    idx=$((idx + 1))
    if [ "$idx" -le "$keep_last" ]; then
      continue
    fi
    [ -f "$path" ] || continue
    if [ "$(stat -c %Y "$path")" -lt "$cutoff" ]; then
      rm -f -- "$path"
    fi
  done
}

mkdir -p "$BACKUP_DIR"

prune_group "$KEEP_LAST_DB" "$KEEP_DAYS" "${BACKUP_DIR}/db"
prune_group "$KEEP_LAST_JSON" "$KEEP_DAYS" "$BACKUP_DIR"

find /tmp -maxdepth 1 -type f \
  \( -name 'activitywatch-*.zip' -o -name 'hayabusa-*.zip' -o -name 'aw-hayabusa-profiles.txt' \) \
  -mtime +0 -delete 2>/dev/null || true

find /tmp -maxdepth 1 -type f \
  \( -name 'aw-worktime-ui-bridge.py' -o -name 'views-default.json' -o -name 'apply_webui_ru_patch.out' \) \
  -mtime +1 -delete 2>/dev/null || true
