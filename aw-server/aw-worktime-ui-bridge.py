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
TIMEOUT = float(os.environ.get("AW_WORKTIME_UI_BRIDGE_TIMEOUT", "60"))
WATCHER_FALLBACK_ENABLED = os.environ.get("AW_WORKTIME_UI_BRIDGE_WATCHER_FALLBACK", "1").strip().lower() not in {
    "0",
    "false",
    "no",
    "off",
}
WATCHER_FALLBACK_STALE_SECONDS = float(os.environ.get("AW_WORKTIME_UI_BRIDGE_WATCHER_STALE_SECONDS", "600"))


SESSIONS_BUCKET = f"aw-worktime-sessions_{HOST}"
AFK_BUCKET = f"aw-rdp-afk_{HOST}"
WINDOW_BUCKET = f"aw-rdp-window_{HOST}"
WATCHER_AFK_BUCKET = f"aw-watcher-afk_{HOST}"
WATCHER_WINDOW_BUCKET = f"aw-watcher-window_{HOST}"
WEB_CATEGORY_BUCKET = f"aw-detmir-web-category_{HOST}"
COLLECTOR_HEALTH_MAX_AGE_SECONDS = float(os.environ.get("AW_WORKTIME_UI_BRIDGE_COLLECTOR_HEALTH_MAX_AGE_SECONDS", "300"))
COLLECTOR_HEALTH_QUERY_LIMIT = int(os.environ.get("AW_WORKTIME_UI_BRIDGE_COLLECTOR_HEALTH_QUERY_LIMIT", "200"))
FOREGROUND_CONTEXT_CACHE_SECONDS = float(os.environ.get("AW_WORKTIME_UI_BRIDGE_FOREGROUND_CACHE_SECONDS", "900"))


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


def get_latest_bucket_event(bucket_id: str):
    try:
        events = _req("GET", f"/api/0/buckets/{bucket_id}/events?limit=1") or []
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise
    if not events:
        return None
    return events[0]


def get_latest_bucket_event_ts(bucket_id: str):
    event = get_latest_bucket_event(bucket_id)
    if not event:
        return None
    ts = event.get("timestamp")
    if not ts:
        return None
    try:
        return parse_iso_utc(ts)
    except Exception:
        return None


def bucket_needs_fallback(bucket_id: str, now_utc: datetime, stale_after_seconds: float):
    latest_dt = get_latest_bucket_event_ts(bucket_id)
    if latest_dt is None:
        return True
    return (now_utc - latest_dt).total_seconds() >= stale_after_seconds


def watcher_window_needs_bridge_sync(now_utc: datetime):
    latest_event = get_latest_bucket_event(WATCHER_WINDOW_BUCKET)
    if latest_event is None:
        return True
    latest_dt = get_latest_bucket_event_ts(WATCHER_WINDOW_BUCKET)
    if latest_dt is None:
        return True
    if (now_utc - latest_dt).total_seconds() >= WATCHER_FALLBACK_STALE_SECONDS:
        return True

    data = latest_event.get("data") or {}
    source = str(data.get("source", "")).strip().lower()
    app = str(data.get("app", "")).strip()
    title = str(data.get("title", "")).strip()
    if source == "aw-worktime-ui-bridge":
        return True
    if source != "aw-worktime-ui-bridge":
        return False
    if app.upper() == "RDP":
        return True
    if title == "RDP idle" or title.startswith("RDP active"):
        return True
    return False


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


def get_latest_active_session_ids(events):
    grouped = {}
    for event in events:
        ts = event.get("timestamp")
        if not ts:
            continue
        grouped.setdefault(ts, []).append(event)
    if not grouped:
        return set()
    latest_ts = max(grouped.keys(), key=lambda item: parse_iso_utc(item))
    active_session_ids = set()
    for event in grouped.get(latest_ts, []):
        data = event.get("data") or {}
        if not _is_session_active(data):
            continue
        try:
            active_session_ids.add(int(data.get("sessionId")))
        except Exception:
            continue
    return active_session_ids


def _normalize_foreground_context(data):
    foreground_process = str(data.get("foregroundProcess", "")).strip()
    foreground_title = str(data.get("foregroundTitle", "")).strip()
    if not foreground_process and not foreground_title:
        return None
    return {
        "app": foreground_process if foreground_process.endswith(".exe") else f"{foreground_process}.exe",
        "title": foreground_title or foreground_process,
    }


def get_latest_foreground_context(now_utc: datetime, active_session_ids=None, state=None):
    try:
        events = _req("GET", f"/api/0/buckets/{WEB_CATEGORY_BUCKET}/events?limit={COLLECTOR_HEALTH_QUERY_LIMIT}") or []
    except urllib.error.HTTPError as e:
        if e.code == 404:
            events = []
        else:
            raise

    active_session_ids = set(active_session_ids or [])
    recent_candidates = []
    for event in reversed(events):
        data = event.get("data") or {}
        if str(data.get("signalType", "")).strip().lower() != "collector_health":
            continue
        ts = event.get("timestamp")
        if not ts:
            continue
        try:
            event_dt = parse_iso_utc(ts)
        except Exception:
            continue
        if (now_utc - event_dt).total_seconds() > COLLECTOR_HEALTH_MAX_AGE_SECONDS:
            continue
        normalized = _normalize_foreground_context(data)
        if not normalized:
            continue
        session_id = data.get("sessionId")
        try:
            session_id = int(session_id)
        except Exception:
            session_id = None
        recent_candidates.append((session_id, event_dt, normalized))

    for session_id, event_dt, normalized in recent_candidates:
        if active_session_ids and session_id in active_session_ids:
            normalized["timestamp"] = to_iso_utc(event_dt.isoformat())
            return normalized

    if recent_candidates:
        session_id, event_dt, normalized = recent_candidates[0]
        normalized["timestamp"] = to_iso_utc(event_dt.isoformat())
        return normalized

    cached = (state or {}).get("last_foreground_context")
    if isinstance(cached, dict):
        cached_ts = str(cached.get("timestamp", "")).strip()
        if cached_ts:
            try:
                cached_dt = parse_iso_utc(cached_ts)
            except Exception:
                cached_dt = None
            if cached_dt and (now_utc - cached_dt).total_seconds() <= FOREGROUND_CONTEXT_CACHE_SECONDS:
                app = str(cached.get("app", "")).strip()
                title = str(cached.get("title", "")).strip()
                if app or title:
                    return {
                        "app": app or "RDP",
                        "title": title or app or "RDP",
                        "timestamp": cached_ts,
                    }
    return None


def _is_session_active(row_data):
    if isinstance(row_data.get("active"), bool):
        if row_data.get("active"):
            return True
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
        session_name = str(row_data.get("sessionName", "")).strip().lower()
        if sid > 0 and user and (not user.endswith("$")) and (session_name.startswith("rdp-") or session_name == "console"):
            return True
    return False


def transform(events, foreground_context=None):
    out_afk = []
    out_win = []
    last_ts = None

    grouped = {}
    for e in events:
        ts = e.get("timestamp")
        if not ts:
            continue
        try:
            normalized_ts = to_iso_utc(parse_iso_utc(ts).replace(microsecond=0).isoformat())
        except Exception:
            normalized_ts = ts
        grouped.setdefault(normalized_ts, []).append(e)

    ordered_ts = sorted(grouped.keys())
    parsed_ts = {}
    for ts in ordered_ts:
        try:
            parsed_ts[ts] = parse_iso_utc(ts)
        except Exception:
            parsed_ts[ts] = None

    for idx, ts in enumerate(ordered_ts):
        rows = grouped[ts]
        src_duration = max(float(r.get("duration", 0.0)) for r in rows)
        duration = src_duration
        cur_dt = parsed_ts.get(ts)
        next_dt = parsed_ts.get(ordered_ts[idx + 1]) if idx + 1 < len(ordered_ts) else None
        next_gap = None
        if cur_dt and next_dt:
            next_gap = max(0.0, (next_dt - cur_dt).total_seconds())
        if duration <= 0:
            if cur_dt and next_dt:
                duration = next_gap or 0.0
            if duration <= 0:
                duration = 10.0
        elif next_gap is not None and next_gap > 0:
            # Do not let a sampled session interval extend past the next sample.
            duration = min(duration, next_gap)
        duration = min(duration, 30.0)
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

        if is_active and foreground_context:
            title = str(foreground_context.get("title") or "").strip()
            if active_count > 1:
                title = f"{title} | {build_window_title(active_users, active_count)}" if title else build_window_title(active_users, active_count)
            win_data = {
                "app": str(foreground_context.get("app") or "RDP"),
                "title": title or build_window_title(active_users, active_count),
                "source": "aw-worktime-ui-bridge",
            }
        else:
            win_data = {
                "app": "RDP",
                "title": build_window_title(active_users, active_count),
                "source": "aw-worktime-ui-bridge",
            }
        out_win.append({"timestamp": ts, "duration": duration, "data": win_data})
        last_ts = ts

    return out_afk, out_win, last_ts


def normalize_watcher_window_events(win_events):
    normalized = []
    for event in win_events:
        cloned = dict(event)
        data = dict(event.get("data") or {})
        app = str(data.get("app") or "").strip()
        title = str(data.get("title") or "").strip()
        if not app or app.upper() == "RDP":
            continue
        if app and app.upper() != "RDP" and " | RDP active (" in title:
            data["title"] = title.split(" | RDP active (", 1)[0].strip()
        cloned["data"] = data
        normalized.append(cloned)
    return normalized


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

    active_session_ids = get_latest_active_session_ids(events)
    foreground_context = get_latest_foreground_context(now_utc, active_session_ids=active_session_ids, state=state)
    afk_events, win_events, new_last_ts = transform(events, foreground_context=foreground_context)
    if not afk_events or not win_events or not new_last_ts:
        return
    watcher_win_events = normalize_watcher_window_events(win_events)

    _req("POST", f"/api/0/buckets/{AFK_BUCKET}/events", afk_events)
    _req("POST", f"/api/0/buckets/{WINDOW_BUCKET}/events", win_events)
    if WATCHER_FALLBACK_ENABLED:
        if bucket_needs_fallback(WATCHER_AFK_BUCKET, now_utc, WATCHER_FALLBACK_STALE_SECONDS):
            ensure_bucket(WATCHER_AFK_BUCKET, "afkstatus", "aw-watcher-afk")
            _req("POST", f"/api/0/buckets/{WATCHER_AFK_BUCKET}/events", afk_events)
        if watcher_win_events and watcher_window_needs_bridge_sync(now_utc):
            ensure_bucket(WATCHER_WINDOW_BUCKET, "currentwindow", "aw-watcher-window")
            _req("POST", f"/api/0/buckets/{WATCHER_WINDOW_BUCKET}/events", watcher_win_events)
    next_state = {"last_ts": new_last_ts}
    if foreground_context:
        next_state["last_foreground_context"] = {
            "app": str(foreground_context.get("app") or ""),
            "title": str(foreground_context.get("title") or ""),
            "timestamp": str(foreground_context.get("timestamp") or to_iso_utc(now_utc.isoformat())),
        }
    elif isinstance(state.get("last_foreground_context"), dict):
        next_state["last_foreground_context"] = state["last_foreground_context"]
    save_state(next_state)
    print(f"posted_afk={len(afk_events)} posted_win={len(win_events)} last_ts={new_last_ts}")


if __name__ == "__main__":
    main()
