#!/bin/bash
set -euo pipefail

ENV_FILE="/etc/activitywatch/aw-server.env"
if [[ ! -f "$ENV_FILE" ]]; then
  echo "missing env file: $ENV_FILE" >&2
  exit 1
fi

source "$ENV_FILE"

WEBUI_DIR="${AW_SERVER_WEBUI_DIR:-${AW_WEBUI_DIR:-/opt/activitywatch/webui-ru}}"
PATCH_JS_SRC="/root/bootstrap/aw-ru-patch.js"
SW_CLEANUP_SRC="/root/bootstrap/aw-sw-cleanup.js"
HOST_GROUPS_SRC="/root/bootstrap/aw-host-groups.json"
INDEX_HTML="$WEBUI_DIR/index.html"
SERVICE_WORKER="$WEBUI_DIR/service-worker.js"
TS=$(date +%Y%m%d%H%M%S)
PATCH_TARGET="$WEBUI_DIR/js/ru-patch-v5.js"
SW_TARGET="$WEBUI_DIR/js/sw-cleanup.js"
HOST_GROUPS_TARGET="$WEBUI_DIR/js/aw-host-groups.json"
TRENDS_NEEDLE='this.activityStore.query_category_time_by_period(r)'
TRENDS_REPLACEMENT='this.activityStore.ensure_loaded(r)'
TIMESPIRAL_NEEDLE='start:new Date("2022-08-08")'
TIMESPIRAL_REPLACEMENT='start:new Date(Date.now()-12*36e5)'
CATEGORY_HELPER_NEEDLE='hostname:t.hostnameChoices[0]'
CATEGORY_HELPER_REPLACEMENT='hostname:t.hostnameChoices.filter((function(t){return"unknown"!==t&&"undefined"!==t}))[0]||t.hostnameChoices[0]'

[[ -f "$PATCH_JS_SRC" ]] || { echo "missing $PATCH_JS_SRC" >&2; exit 1; }
[[ -f "$SW_CLEANUP_SRC" ]] || { echo "missing $SW_CLEANUP_SRC" >&2; exit 1; }
[[ -f "$HOST_GROUPS_SRC" ]] || { echo "missing $HOST_GROUPS_SRC" >&2; exit 1; }
[[ -f "$INDEX_HTML" ]] || { echo "missing $INDEX_HTML" >&2; exit 1; }

install -d "$WEBUI_DIR/js"
install -m 0644 "$PATCH_JS_SRC" "$PATCH_TARGET"
install -m 0644 "$SW_CLEANUP_SRC" "$SW_TARGET"
install -m 0644 "$HOST_GROUPS_SRC" "$HOST_GROUPS_TARGET"
cp "$INDEX_HTML" "$INDEX_HTML.bak.$TS"

patch_hash="$(sha1sum "$PATCH_TARGET" | awk '{print substr($1,1,12)}')"
sw_hash="$(sha1sum "$SW_TARGET" | awk '{print substr($1,1,12)}')"

sed -i '/ru-patch-v5.js/d;/sw-cleanup.js/d;/aw-ru-patch.js/d;/aw-sw-cleanup.js/d' "$INDEX_HTML"
sed -i "s#</head>#<script src=\"/js/sw-cleanup.js?v=$sw_hash\"></script></head>#" "$INDEX_HTML"
sed -i "s#</body>#<script defer=\"defer\" src=\"/js/ru-patch-v5.js?v=$patch_hash\"></script></body>#" "$INDEX_HTML"
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

category_helper_chunk="$(grep -Rsl "$CATEGORY_HELPER_NEEDLE" "$WEBUI_DIR/js"/*.js 2>/dev/null | head -n 1 || true)"
if [[ -n "$category_helper_chunk" ]]; then
  cp "$category_helper_chunk" "$category_helper_chunk.bak.$TS"
  python3 - "$category_helper_chunk" "$CATEGORY_HELPER_NEEDLE" "$CATEGORY_HELPER_REPLACEMENT" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old = sys.argv[2]
new = sys.argv[3]
content = path.read_text()
if old in content:
    path.write_text(content.replace(old, new, 1))
    print(f"Category helper host hotfix applied to {path}")
else:
    print(f"Category helper host hotfix already present in {path}")
PY
else
  echo "Category helper host hotfix skipped: chunk not found"
fi

echo "RU patch applied to $WEBUI_DIR (ru-patch-v5.js?v=$patch_hash)"
