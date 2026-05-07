#!/usr/bin/env python3
import json
import os
import urllib.error
import urllib.request
from datetime import datetime, timezone


AW_URL = os.environ.get("AW_SERVER_URL", "http://127.0.0.1:5600")
HOST = os.environ.get("AW_WORKTIME_HOST", "SHARKON2025")
STATE_PATH = os.environ.get(
    "AW_WORKTIME_UI_BRIDGE_STATE",
    "/var/lib/activitywatch/aw-worktime-ui-bridge-state.json",
)
TIMEOUT = float(os.environ.get("AW_WORKTIME_UI_BRIDGE_TIMEOUT", "20"))


SESSIONS_BUCKET = f"aw-worktime-sessions_{HOST}"
AFK_BUCKET = f"aw-watcher-afk_{HOST}"
WINDOW_BUCKET = f"aw-watcher-window_{HOST}"


def _req(method: str, path: str, payload=None):
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(AW_URL + path, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        raw = r.read()
        if not raw:
            return None
        return json.loads(raw.decode("utf-8"))


def ensure_bucket(bucket_id: str, event_type: str, client: str):
    payload = {"client": client, "type": event_type, "hostname": HOST}
    try:
        _req("POST", f"/api/0/buckets/{bucket_id}", payload)
    except urllib.error.HTTPError as e:
        if e.code != 304:
            raise


def load_state():
    try:
        with open(STATE_PATH, "r", encoding="utf-8") as f:
            data = json.load(f)
            if isinstance(data, dict) and "last_ts" in data:
                return data
    except FileNotFoundError:
        pass
    except json.JSONDecodeError:
        pass
    return {"last_ts": "1970-01-01T00:00:00Z"}


def save_state(state):
    os.makedirs(os.path.dirname(STATE_PATH), exist_ok=True)
    tmp = STATE_PATH + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(state, f, ensure_ascii=False)
    os.replace(tmp, STATE_PATH)


def to_iso_utc(ts):
    if ts.endswith("Z"):
        return ts
    return ts.replace("+00:00", "Z")


def build_window_title(users, active_count):
    if not users:
        return "RDP idle"
    return f"RDP active ({active_count}): " + ", ".join(users)


def transform(events):
    out_afk = []
    out_win = []
    last_ts = None

    for e in events:
        ts = e.get("timestamp")
        if not ts:
            continue
        duration = float(e.get("duration", 0.0))
        data = e.get("data") or {}
        active_users = data.get("activeUsers") or []
        active_count = int(data.get("activeCount", len(active_users)))
        is_active = active_count > 0

        afk_data = {"status": "not-afk" if is_active else "afk", "source": "aw-worktime-ui-bridge"}
        out_afk.append({"timestamp": ts, "duration": duration, "data": afk_data})

        win_data = {
            "app": "RDP",
            "title": build_window_title(active_users, active_count),
            "source": "aw-worktime-ui-bridge",
        }
        out_win.append({"timestamp": ts, "duration": duration, "data": win_data})
        last_ts = ts

    return out_afk, out_win, last_ts


def main():
    state = load_state()
    last_ts = state.get("last_ts", "1970-01-01T00:00:00Z")

    ensure_bucket(AFK_BUCKET, "afkstatus", "aw-worktime-ui-bridge")
    ensure_bucket(WINDOW_BUCKET, "currentwindow", "aw-worktime-ui-bridge")

    query = {
        "query": [
            "events = query_bucket(find_bucket($bid));",
            "RETURN = sort_by_timestamp(events);",
        ],
        "timeperiods": [[last_ts, to_iso_utc(datetime.now(timezone.utc).isoformat())]],
    }
    rows = _req("POST", f"/api/0/query/?bid={SESSIONS_BUCKET}", query) or []
    if not rows or not rows[0]:
        return

    events = rows[0]
    afk_events, win_events, new_last_ts = transform(events)
    if not afk_events or not win_events or not new_last_ts:
        return

    _req("POST", f"/api/0/buckets/{AFK_BUCKET}/events", afk_events)
    _req("POST", f"/api/0/buckets/{WINDOW_BUCKET}/events", win_events)
    save_state({"last_ts": new_last_ts})
    print(f"posted_afk={len(afk_events)} posted_win={len(win_events)} last_ts={new_last_ts}")


if __name__ == "__main__":
    main()
