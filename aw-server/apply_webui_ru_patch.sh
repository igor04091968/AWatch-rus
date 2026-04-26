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
TRENDS_NEEDLE='this.activityStore.query_category_time_by_period(r)'
TRENDS_REPLACEMENT='this.activityStore.ensure_loaded(r)'
TIMESPIRAL_NEEDLE='start:new Date("2022-08-08")'
TIMESPIRAL_REPLACEMENT='start:new Date(Date.now()-12*36e5)'

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

trends_chunk="$(grep -Rsl "$TRENDS_NEEDLE" "$WEBUI_DIR/js"/*.js 2>/dev/null | head -n 1 || true)"
if [[ -n "$trends_chunk" ]]; then
  cp "$trends_chunk" "$trends_chunk.bak.$TS"
  python3 - "$trends_chunk" "$TRENDS_NEEDLE" "$TRENDS_REPLACEMENT" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old = sys.argv[2]
new = sys.argv[3]
content = path.read_text()
if old in content:
    path.write_text(content.replace(old, new, 1))
    print(f"Trends hotfix applied to {path}")
else:
    print(f"Trends hotfix already present in {path}")
PY
else
  echo "Trends hotfix skipped: chunk not found"
fi

timespiral_chunk="$(grep -Rsl "$TIMESPIRAL_NEEDLE" "$WEBUI_DIR/js"/*.js 2>/dev/null | head -n 1 || true)"
if [[ -n "$timespiral_chunk" ]]; then
  cp "$timespiral_chunk" "$timespiral_chunk.bak.$TS"
  python3 - "$timespiral_chunk" "$TIMESPIRAL_NEEDLE" "$TIMESPIRAL_REPLACEMENT" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old = sys.argv[2]
new = sys.argv[3]
content = path.read_text()
if old in content:
    path.write_text(content.replace(old, new, 1))
    print(f"Timespiral hotfix applied to {path}")
else:
    print(f"Timespiral hotfix already present in {path}")
PY
else
  echo "Timespiral hotfix skipped: chunk not found"
fi

echo "RU patch applied to $WEBUI_DIR"
