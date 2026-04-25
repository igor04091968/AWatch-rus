#!/bin/bash
set -euo pipefail

ENV_FILE="/etc/activitywatch/aw-server.env"
if [[ ! -f "$ENV_FILE" ]]; then
  echo "missing env file: $ENV_FILE" >&2
  exit 1
fi

source "$ENV_FILE"

WEBUI_DIR="${AW_SERVER_WEBUI_DIR:-/opt/activitywatch/webui-ru}"
PATCH_JS_SRC="/root/bootstrap/aw-ru-patch.js"
SW_CLEANUP_SRC="/root/bootstrap/aw-sw-cleanup.js"
INDEX_HTML="$WEBUI_DIR/index.html"
SERVICE_WORKER="$WEBUI_DIR/service-worker.js"
TS=$(date +%Y%m%d%H%M%S)

[[ -f "$PATCH_JS_SRC" ]] || { echo "missing $PATCH_JS_SRC" >&2; exit 1; }
[[ -f "$SW_CLEANUP_SRC" ]] || { echo "missing $SW_CLEANUP_SRC" >&2; exit 1; }
[[ -f "$INDEX_HTML" ]] || { echo "missing $INDEX_HTML" >&2; exit 1; }

install -d "$WEBUI_DIR/js"
install -m 0644 "$PATCH_JS_SRC" "$WEBUI_DIR/js/aw-ru-patch.js"
install -m 0644 "$SW_CLEANUP_SRC" "$WEBUI_DIR/js/aw-sw-cleanup.js"
cp "$INDEX_HTML" "$INDEX_HTML.bak.$TS"

sed -i '/aw-ru-patch.js/d;/aw-sw-cleanup.js/d' "$INDEX_HTML"
sed -i 's#</head>#<script src="/js/aw-sw-cleanup.js"></script></head>#' "$INDEX_HTML"
sed -i 's#</body>#<script defer="defer" src="/js/aw-ru-patch.js"></script></body>#' "$INDEX_HTML"
cp "$SW_CLEANUP_SRC" "$SERVICE_WORKER"

echo "RU patch applied to $WEBUI_DIR"
