#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${AW_WORKFORCE_ROOT:-/opt/activitywatch/clickhouse-workforce}"
ENV_DIR="${AW_WORKFORCE_ENV_DIR:-/etc/activitywatch}"
STATE_DIR="${AW_WORKFORCE_STATE_DIR:-/var/lib/aw-workforce-ingest}"
SYSTEMD_DIR="${AW_WORKFORCE_SYSTEMD_DIR:-/etc/systemd/system}"

install -d -m 0755 "$ROOT" "$ENV_DIR" "$STATE_DIR" "$SYSTEMD_DIR"

if [[ ! -f "$ENV_DIR/aw-workforce-ingest.env" ]]; then
  install -m 0640 "$SCRIPT_DIR/aw-workforce-ingest.env.example" \
    "$ENV_DIR/aw-workforce-ingest.env"
fi

install -m 0644 "$SCRIPT_DIR/aw-workforce-ingest.service" \
  "$SYSTEMD_DIR/aw-workforce-ingest.service"
install -m 0644 "$SCRIPT_DIR/aw-workforce-ingest.timer" \
  "$SYSTEMD_DIR/aw-workforce-ingest.timer"

if command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload
fi

cat <<EOF
Installed aw-workforce-ingest runtime files.

Next manual deployment steps:
  install -m 0755 <built aw-workforce-ingest binary> /usr/local/bin/aw-workforce-ingest
  edit $ENV_DIR/aw-workforce-ingest.env
  systemctl enable --now aw-workforce-ingest.timer
  systemctl start aw-workforce-ingest.service
EOF
