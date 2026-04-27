#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
DEFAULT_ENV_FILE="$PROJECT_ROOT/secrets/deploy.secrets.env"

if [ "$#" -gt 1 ]; then
  echo "usage: $0 [ /path/to/deploy.secrets.env ]" >&2
  exit 1
fi

ENV_FILE="${1:-$DEFAULT_ENV_FILE}"

# shellcheck disable=SC1090
. "$ENV_FILE"

: "${CT_ID:?missing CT_ID}"

pct exec "$CT_ID" -- mkdir -p /root/bootstrap

for file_name in \
  install_aw_server.sh \
  apply_webui_ru_patch.sh \
  activitywatch-server.service \
  aw-server.env.example \
  aw-ru-patch.js \
  aw-sw-cleanup.js \
  aw-host-groups.json
do
  pct push "$CT_ID" "$PROJECT_ROOT/aw-server/$file_name" "/root/bootstrap/$file_name"
done

pct exec "$CT_ID" -- mkdir -p /root/bootstrap/settings
pct push "$CT_ID" "$PROJECT_ROOT/aw-server/settings/classes-worktime.json" "/root/bootstrap/settings/classes-worktime.json"
pct push "$CT_ID" "$PROJECT_ROOT/aw-server/settings/views-default.json" "/root/bootstrap/settings/views-default.json"

if [ -n "${AW_SERVER_VERSION:-}" ] &&
   [ -n "${AW_SERVER_DOWNLOAD_URL:-}" ] &&
   [ -n "${AW_SERVER_BIND_HOST:-}" ] &&
   [ -n "${AW_SERVER_PORT:-}" ] &&
   [ -n "${AW_SERVER_WEBUI_DIR:-}" ] &&
   [ -n "${AW_SERVER_DATA_DIR:-}" ] &&
   [ -n "${AW_SERVER_LOG_DIR:-}" ] &&
   [ -n "${AW_SERVER_USER:-}" ] &&
   [ -n "${AW_SERVER_GROUP:-}" ]; then
  tmp_env=$(mktemp)
  trap 'rm -f "$tmp_env"' EXIT
  {
    echo "AW_SERVER_VERSION=$AW_SERVER_VERSION"
    echo "AW_SERVER_DOWNLOAD_URL=$AW_SERVER_DOWNLOAD_URL"
    echo "AW_SERVER_BIND_HOST=$AW_SERVER_BIND_HOST"
    echo "AW_SERVER_PORT=$AW_SERVER_PORT"
    echo "AW_SERVER_WEBUI_DIR=$AW_SERVER_WEBUI_DIR"
    echo "AW_SERVER_DATA_DIR=$AW_SERVER_DATA_DIR"
    echo "AW_SERVER_LOG_DIR=$AW_SERVER_LOG_DIR"
    echo "AW_SERVER_USER=$AW_SERVER_USER"
    echo "AW_SERVER_GROUP=$AW_SERVER_GROUP"
  } > "$tmp_env"
  pct push "$CT_ID" "$tmp_env" "/etc/activitywatch/aw-server.env"
  pct exec "$CT_ID" -- chmod 0600 /etc/activitywatch/aw-server.env
  echo "Server env pushed to CT $CT_ID:/etc/activitywatch/aw-server.env"
else
  echo "WARN: AW_SERVER_* variables are incomplete in $ENV_FILE; /etc/activitywatch/aw-server.env was not updated" >&2
fi

echo "Bootstrap artifacts pushed to CT $CT_ID:/root/bootstrap"
