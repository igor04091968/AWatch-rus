#!/bin/bash
set -euo pipefail

ENV_FILE="/etc/activitywatch/aw-server.env"
if [[ ! -f "$ENV_FILE" ]]; then
  echo "missing env file: $ENV_FILE" >&2
  exit 1
fi

source "$ENV_FILE"

required_vars=(
  AW_SERVER_VERSION
  AW_SERVER_DOWNLOAD_URL
  AW_SERVER_BIND_HOST
  AW_SERVER_PORT
  AW_SERVER_WEBUI_DIR
  AW_SERVER_DATA_DIR
  AW_SERVER_LOG_DIR
  AW_SERVER_USER
  AW_SERVER_GROUP
)

BOOTSTRAP_DIR="/root/bootstrap"
VIEWS_JSON="$BOOTSTRAP_DIR/settings/views-default.json"
CLASSES_JSON="$BOOTSTRAP_DIR/settings/classes-worktime.json"
WORKTIME_API_SRC="$BOOTSTRAP_DIR/aw-worktime-api.py"
WORKTIME_API_SERVICE_SRC="$BOOTSTRAP_DIR/aw-worktime-api.service"
WORKTIME_UI_BRIDGE_SRC="$BOOTSTRAP_DIR/aw-worktime-ui-bridge.py"
WORKTIME_UI_BRIDGE_SERVICE_SRC="$BOOTSTRAP_DIR/aw-worktime-ui-bridge.service"
WORKTIME_UI_BRIDGE_TIMER_SRC="$BOOTSTRAP_DIR/aw-worktime-ui-bridge.timer"
HEALTHD_SRC="$BOOTSTRAP_DIR/aw-rus-healthd.py"
HEALTHD_SERVICE_SRC="$BOOTSTRAP_DIR/aw-rus-healthd.service"
HEALTHD_TIMER_SRC="$BOOTSTRAP_DIR/aw-rus-healthd.timer"

for var_name in "${required_vars[@]}"; do
  if [[ -z "${!var_name:-}" ]]; then
    echo "missing required variable: $var_name" >&2
    exit 1
  fi
done

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y curl ca-certificates unzip jq

if ! getent group "$AW_SERVER_GROUP" >/dev/null; then
  groupadd --system "$AW_SERVER_GROUP"
fi

if ! id "$AW_SERVER_USER" >/dev/null 2>&1; then
  useradd --system --gid "$AW_SERVER_GROUP" --home-dir "$AW_SERVER_DATA_DIR" --shell /usr/sbin/nologin "$AW_SERVER_USER"
fi

install -d -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" /opt/activitywatch/bin
install -d -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" /opt/activitywatch/releases
install -d -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" "$AW_SERVER_WEBUI_DIR"
install -d -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" "$AW_SERVER_DATA_DIR"
install -d -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" "$AW_SERVER_LOG_DIR"
install -d -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" "$AW_SERVER_DATA_DIR/health/windows-validation"

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

curl -fL "$AW_SERVER_DOWNLOAD_URL" -o "$tmp_dir/aw-server.zip"
unzip -q "$tmp_dir/aw-server.zip" -d "$tmp_dir/unpacked"

server_bin=$(find "$tmp_dir/unpacked" -type f \( -name 'aw-server-rust' -o -name 'aw-server' \) | head -n 1)
webui_dir=$(find "$tmp_dir/unpacked" -type d \( -name 'webui' -o -name 'aw-webui' \) | head -n 1 || true)

if [[ -z "$server_bin" || ! -f "$server_bin" ]]; then
  echo "aw-server binary not found in archive" >&2
  exit 1
fi

release_dir="/opt/activitywatch/releases/aw-server-rust-v${AW_SERVER_VERSION}"
rm -rf "$release_dir"
install -d -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" "$release_dir"
install -m 0755 -o "$AW_SERVER_USER" -g "$AW_SERVER_GROUP" "$server_bin" "$release_dir/aw-server-rust"
ln -sfn "$release_dir/aw-server-rust" /opt/activitywatch/bin/aw-server-rust

if [[ -n "$webui_dir" && -d "$webui_dir" ]]; then
  rm -rf "$AW_SERVER_WEBUI_DIR"
  mkdir -p "$AW_SERVER_WEBUI_DIR"
  cp -a "$webui_dir"/. "$AW_SERVER_WEBUI_DIR"/
  chown -R "$AW_SERVER_USER:$AW_SERVER_GROUP" "$AW_SERVER_WEBUI_DIR"
fi

sed \
  -e "s#__AW_SERVER_USER__#$AW_SERVER_USER#g" \
  -e "s#__AW_SERVER_GROUP__#$AW_SERVER_GROUP#g" \
  -e "s#__AW_SERVER_DATA_DIR__#$AW_SERVER_DATA_DIR#g" \
  /root/bootstrap/activitywatch-server.service > /etc/systemd/system/activitywatch-server.service
chmod 0644 /etc/systemd/system/activitywatch-server.service

systemctl daemon-reload
systemctl enable activitywatch-server.service
systemctl restart activitywatch-server.service
systemctl --no-pager --full status activitywatch-server.service || true

if [[ -f "$WORKTIME_API_SRC" ]]; then
  install -m 0755 "$WORKTIME_API_SRC" /usr/local/bin/aw-worktime-api.py
fi

if [[ -f "$WORKTIME_API_SERVICE_SRC" ]]; then
  install -m 0644 "$WORKTIME_API_SERVICE_SRC" /etc/systemd/system/aw-worktime-api.service
  systemctl daemon-reload
  systemctl enable aw-worktime-api.service
  systemctl restart aw-worktime-api.service
  systemctl --no-pager --full status aw-worktime-api.service || true
fi

if [[ -f "$WORKTIME_UI_BRIDGE_SRC" ]]; then
  install -m 0755 "$WORKTIME_UI_BRIDGE_SRC" /usr/local/bin/aw-worktime-ui-bridge.py
fi

if [[ -f "$WORKTIME_UI_BRIDGE_SERVICE_SRC" ]]; then
  install -m 0644 "$WORKTIME_UI_BRIDGE_SERVICE_SRC" /etc/systemd/system/aw-worktime-ui-bridge.service
fi

if [[ -f "$WORKTIME_UI_BRIDGE_TIMER_SRC" ]]; then
  install -m 0644 "$WORKTIME_UI_BRIDGE_TIMER_SRC" /etc/systemd/system/aw-worktime-ui-bridge.timer
  systemctl daemon-reload
  systemctl disable --now aw-worktime-afk-bridge.timer >/dev/null 2>&1 || true
  systemctl enable aw-worktime-ui-bridge.timer
  systemctl restart aw-worktime-ui-bridge.timer
  systemctl start aw-worktime-ui-bridge.service || true
  systemctl --no-pager --full status aw-worktime-ui-bridge.timer || true
fi

if [[ -f "$HEALTHD_SRC" ]]; then
  install -m 0755 "$HEALTHD_SRC" /usr/local/bin/aw-rus-healthd.py
fi

if [[ -f "$HEALTHD_SERVICE_SRC" ]]; then
  install -m 0644 "$HEALTHD_SERVICE_SRC" /etc/systemd/system/aw-rus-healthd.service
fi

if [[ -f "$HEALTHD_TIMER_SRC" ]]; then
  install -m 0644 "$HEALTHD_TIMER_SRC" /etc/systemd/system/aw-rus-healthd.timer
  systemctl daemon-reload
  systemctl enable aw-rus-healthd.timer
  systemctl restart aw-rus-healthd.timer
  systemctl start aw-rus-healthd.service || true
  systemctl --no-pager --full status aw-rus-healthd.timer || true
fi

for _ in $(seq 1 20); do
  if curl -fsS "http://127.0.0.1:${AW_SERVER_PORT}/api/0/info" >/dev/null 2>&1; then
    break
  fi
  sleep 2
done

if [[ -f "$CLASSES_JSON" ]]; then
  curl -fsS -X POST \
    -H 'Content-Type: application/json' \
    --data-binary @"$CLASSES_JSON" \
    "http://127.0.0.1:${AW_SERVER_PORT}/api/0/settings/classes" >/dev/null
  echo "Applied worktime classes from $CLASSES_JSON"
else
  echo "Worktime classes bootstrap not found, skipped: $CLASSES_JSON"
fi

if [[ -f "$VIEWS_JSON" ]]; then
  curl -fsS -X POST \
    -H 'Content-Type: application/json' \
    --data-binary @"$VIEWS_JSON" \
    "http://127.0.0.1:${AW_SERVER_PORT}/api/0/settings/views" >/dev/null
  echo "Applied baseline views from $VIEWS_JSON"
else
  echo "Views bootstrap not found, skipped: $VIEWS_JSON"
fi
