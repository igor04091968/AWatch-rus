#!/usr/bin/env python3
import json
import os
import urllib.error
import urllib.request
from datetime import datetime, timedelta, timezone


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


def parse_iso_utc(ts: str):
    if ts.endswith("Z"):
        ts = ts[:-1] + "+00:00"
    return datetime.fromisoformat(ts).astimezone(timezone.utc)


def build_window_title(users, active_count):
    if not users:
        return "RDP idle"
    return f"RDP active ({active_count}): " + ", ".join(users)


def _is_session_active(row_data):
    if isinstance(row_data.get("active"), bool):
        return row_data.get("active")
    state = str(row_data.get("state", "")).strip().lower()
    if state in {"active", "активно"}:
        return True
    # query user can intermittently return Unknown on RDP hosts.
    if state == "unknown":
        try:
            sid = int(row_data.get("sessionId"))
        except Exception:
            sid = -1
        user = str(row_data.get("username", "")).strip().lower()
        if sid > 0 and user and (not user.endswith("$")):
            return True
    return False


def transform(events):
    out_afk = []
    out_win = []
    last_ts = None

    grouped = {}
    for e in events:
        ts = e.get("timestamp")
        if not ts:
            continue
        grouped.setdefault(ts, []).append(e)

    for ts in sorted(grouped.keys()):
        rows = grouped[ts]
        duration = max(float(r.get("duration", 0.0)) for r in rows)
        active_users = []
        for r in rows:
            data = r.get("data") or {}
            user = str(data.get("username", "")).strip()
            if user and _is_session_active(data):
                active_users.append(user)
        active_users = sorted(set(active_users))
        active_count = len(active_users)
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

    now_utc = datetime.now(timezone.utc)
    recent = _req("GET", f"/api/0/buckets/{SESSIONS_BUCKET}/events?limit=5000") or []
    if not recent:
        return

    try:
        last_dt = parse_iso_utc(last_ts)
    except Exception:
        last_dt = datetime(1970, 1, 1, tzinfo=timezone.utc)

    events = []
    for e in recent:
        ts = e.get("timestamp")
        if not ts:
            continue
        try:
            if parse_iso_utc(ts) > last_dt:
                events.append(e)
        except Exception:
            continue
    if not events:
        return

    afk_events, win_events, new_last_ts = transform(events)
    if not afk_events or not win_events or not new_last_ts:
        return

    _req("POST", f"/api/0/buckets/{AFK_BUCKET}/events", afk_events)
    _req("POST", f"/api/0/buckets/{WINDOW_BUCKET}/events", win_events)
    save_state({"last_ts": new_last_ts})
    print(f"posted_afk={len(afk_events)} posted_win={len(win_events)} last_ts={new_last_ts}")


if __name__ == "__main__":
    main()
