#!/usr/bin/env python3
import csv
import html
import io
import importlib.util
import json
import os
import sys
import tempfile
import threading
import urllib.request
from datetime import datetime, timezone, timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlparse
from zoneinfo import ZoneInfo


def build_aw_api_base(raw_url):
    url = (raw_url or "http://127.0.0.1:5600").strip().rstrip("/")
    if url.endswith("/api/0"):
        return url
    return url + "/api/0"


AW_SERVER_URL = os.environ.get("AW_SERVER_URL", "http://127.0.0.1:5600")
AW = build_aw_api_base(AW_SERVER_URL)
REPORT_TZ = ZoneInfo(os.environ.get("AW_WORKTIME_TZ", "Europe/Moscow"))
IOC_DIR = os.environ.get("AW_DLP_IOC_DIR", "/opt/activitywatch/dlp-ioc/output")
DEFAULT_HOST = os.environ.get("AW_WORKTIME_HOST", "SHARKON2025").strip() or "SHARKON2025"
DEFAULT_SAMPLE_SECONDS = max(1.0, float(os.environ.get("AW_WORKTIME_DEFAULT_SAMPLE_SECONDS", "30")))
MAX_SAMPLE_SECONDS = max(DEFAULT_SAMPLE_SECONDS, float(os.environ.get("AW_WORKTIME_MAX_SAMPLE_SECONDS", "300")))
LISTEN_HOST = os.environ.get("AW_WORKTIME_LISTEN_HOST", "0.0.0.0")
LISTEN_PORT = int(os.environ.get("AW_WORKTIME_PORT", "5610"))
WORKDAY_START_HOUR = int(os.environ.get("AW_WORKTIME_MANAGER_START_HOUR", "9"))
WORKDAY_END_HOUR = int(os.environ.get("AW_WORKTIME_MANAGER_END_HOUR", "18"))
MANAGER_TARGET_COVERAGE_PCT = max(1, min(100, int(os.environ.get("AW_WORKTIME_MANAGER_TARGET_COVERAGE_PCT", "75"))))
MANAGER_LOW_COVERAGE_PCT = max(1, min(100, int(os.environ.get("AW_WORKTIME_MANAGER_LOW_COVERAGE_PCT", "35"))))
MANAGER_LATE_START_GRACE_MINUTES = max(0, int(os.environ.get("AW_WORKTIME_MANAGER_LATE_START_GRACE_MINUTES", "60")))
MANAGER_EARLY_FINISH_GRACE_MINUTES = max(0, int(os.environ.get("AW_WORKTIME_MANAGER_EARLY_FINISH_GRACE_MINUTES", "90")))
MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS = max(60, int(os.environ.get("AW_WORKTIME_MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS", "900")))
MANAGER_WEB_SOURCE_MAX_AGE_SECONDS = max(3600, int(os.environ.get("AW_WORKTIME_MANAGER_WEB_SOURCE_MAX_AGE_SECONDS", "259200")))
MANAGER_SESSION_SOURCE_MAX_AGE_SECONDS = max(3600, int(os.environ.get("AW_WORKTIME_MANAGER_SESSION_SOURCE_MAX_AGE_SECONDS", "604800")))
MANAGER_INFRA_SOURCE_MAX_AGE_SECONDS = max(3600, int(os.environ.get("AW_WORKTIME_MANAGER_INFRA_SOURCE_MAX_AGE_SECONDS", "172800")))
MANAGER_TREND_DAYS = max(3, min(31, int(os.environ.get("AW_WORKTIME_MANAGER_TREND_DAYS", "7"))))
MANAGER_CACHE_TTL_SECONDS = max(0, int(os.environ.get("AW_WORKTIME_MANAGER_CACHE_TTL_SECONDS", "300")))
MANAGER_CACHE_DIR = Path(os.environ.get("AW_WORKTIME_MANAGER_CACHE_DIR", "/var/lib/activitywatch/worktime-cache"))
MANAGER_ALIASES_JSON = Path(os.environ.get("AW_WORKTIME_MANAGER_ALIASES_JSON", "/etc/activitywatch/worktime-manager-aliases.json"))
MANAGER_EXCLUDE_USERS = {item.strip().lower() for item in os.environ.get("AW_WORKTIME_MANAGER_EXCLUDE_USERS", "").split(",") if item.strip()}
EVENTS_CACHE_TTL_SECONDS = max(0, int(os.environ.get("AW_WORKTIME_EVENTS_CACHE_TTL_SECONDS", "30")))
WORKTIME_EVENTS_LIMIT = max(1000, int(os.environ.get("AW_WORKTIME_EVENTS_LIMIT", "50000")))
TRUE_ACTIVE_EVIDENCE_WINDOW_SECONDS = max(30, int(os.environ.get("AW_WORKTIME_TRUE_ACTIVE_EVIDENCE_WINDOW_SECONDS", "180")))
TRUE_ACTIVE_MAX_EVENT_SECONDS = max(30, int(os.environ.get("AW_WORKTIME_TRUE_ACTIVE_MAX_EVENT_SECONDS", "600")))
MODULE_PATH = Path(__file__).resolve()
_ALIASES_CACHE = {"mtime": None, "users": {}, "owners": {}, "raw": {}}
_EVENTS_CACHE_LOCK = threading.Lock()
_EVENTS_CACHE = {}
_MANAGEMENT_BUILD_LOCKS_LOCK = threading.Lock()
_MANAGEMENT_BUILD_LOCKS = {}


def get(u):
    with urllib.request.urlopen(u, timeout=30) as r:
        return json.loads(r.read().decode())


def log_warning(message):
    print(f"[aw-worktime-api] {message}", file=sys.stderr, flush=True)


def pts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)


def to_iso_utc(dt):
    return dt.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def parse_iso_utc(value):
    if not value:
        return None
    return pts(value)


def hhmm(total_seconds):
    total_seconds = max(0, int(total_seconds))
    return "%02d:%02d" % (total_seconds // 3600, (total_seconds % 3600) // 60)


def human_duration_ru(total_seconds):
    total_seconds = max(0, int(total_seconds))
    hours = total_seconds // 3600
    minutes = (total_seconds % 3600) // 60
    if hours and minutes:
        return f"{hours} ч {minutes} мин"
    if hours:
        return f"{hours} ч"
    if minutes:
        return f"{minutes} мин"
    return f"{total_seconds} сек"


def now_utc():
    return datetime.now(timezone.utc)


def worktime_health_payload():
    return {
        "ok": True,
        "generated_at_utc": now_utc().isoformat().replace("+00:00", "Z"),
        "report_timezone": str(REPORT_TZ),
        "default_host": DEFAULT_HOST,
        "aw_api_base": AW,
    }


def age_seconds(ts, now=None):
    if ts is None:
        return None
    ref = now or now_utc()
    return max(0, int((ref - ts).total_seconds()))


def write_atomic_json(path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as handle:
        json.dump(payload, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
        tmp_name = handle.name
    os.replace(tmp_name, path)


def safe_slug(value):
    text = str(value or "").strip().lower()
    slug = []
    for char in text:
        if char.isalnum():
            slug.append(char)
        else:
            slug.append("-")
    normalized = "".join(slug).strip("-")
    while "--" in normalized:
        normalized = normalized.replace("--", "-")
    return normalized or "user"


def _normalize_identity_key(value):
    return str(value or "").strip().lower()


def _default_display_name(user, user_id):
    base = str(user_id or user or "").strip()
    if "\\" in base:
        base = base.split("\\", 1)[1]
    if not base:
        base = str(user or "").strip()
    if base and base.isascii() and base.lower() == base and any(ch.isalpha() for ch in base):
        return base.upper()
    return base or str(user or "").strip()


def _load_manager_directory():
    try:
        stat = MANAGER_ALIASES_JSON.stat()
    except FileNotFoundError:
        _ALIASES_CACHE["mtime"] = None
        _ALIASES_CACHE["users"] = {}
        _ALIASES_CACHE["owners"] = {}
        _ALIASES_CACHE["raw"] = {}
        return {"users": {}, "owners": {}, "raw": {}}
    mtime = stat.st_mtime
    if _ALIASES_CACHE["mtime"] == mtime:
        return {
            "users": _ALIASES_CACHE["users"],
            "owners": _ALIASES_CACHE["owners"],
            "raw": _ALIASES_CACHE["raw"],
        }
    try:
        raw = json.loads(MANAGER_ALIASES_JSON.read_text(encoding="utf-8"))
    except Exception as exc:
        log_warning(f"failed to load manager aliases from {MANAGER_ALIASES_JSON}: {exc}")
        raw = {}
    users_payload = {}
    owners_payload = {}
    if isinstance(raw, dict):
        users = raw.get("users", raw)
        if isinstance(users, dict):
            for key, value in users.items():
                norm_key = _normalize_identity_key(key)
                if not norm_key:
                    continue
                if isinstance(value, str):
                    users_payload[norm_key] = {"display_name": value}
                elif isinstance(value, dict):
                    users_payload[norm_key] = dict(value)
        owners = raw.get("owners", {})
        if isinstance(owners, dict):
            for key, value in owners.items():
                norm_key = _normalize_identity_key(key)
                if not norm_key:
                    continue
                if isinstance(value, str):
                    owners_payload[norm_key] = {"display_name": value}
                elif isinstance(value, dict):
                    owners_payload[norm_key] = dict(value)
    _ALIASES_CACHE["mtime"] = mtime
    _ALIASES_CACHE["users"] = users_payload
    _ALIASES_CACHE["owners"] = owners_payload
    _ALIASES_CACHE["raw"] = raw if isinstance(raw, dict) else {}
    return {
        "users": users_payload,
        "owners": owners_payload,
        "raw": _ALIASES_CACHE["raw"],
    }


def load_manager_aliases():
    return _load_manager_directory()["users"]


def load_manager_owners():
    return _load_manager_directory()["owners"]


def resolve_user_alias(user, user_id, host):
    aliases = load_manager_aliases()
    candidates = [
        _normalize_identity_key(user_id),
        _normalize_identity_key(f"{resolve_host(host)}\\{user}"),
        _normalize_identity_key(user),
    ]
    alias = {}
    for candidate in candidates:
        if candidate and candidate in aliases:
            alias = dict(aliases[candidate])
            break
    display_name = str(alias.get("display_name") or alias.get("name") or _default_display_name(user, user_id)).strip()
    manager_owner = str(alias.get("manager") or alias.get("owner") or display_name).strip() or display_name
    department = str(alias.get("department") or "").strip()
    role = str(alias.get("role") or "").strip()
    notes = str(alias.get("notes") or "").strip()
    canonical_user_id = str(alias.get("canonical_user_id") or user_id or "").strip()
    exclude = bool(alias.get("exclude")) or _normalize_identity_key(user) in MANAGER_EXCLUDE_USERS or _normalize_identity_key(display_name) in MANAGER_EXCLUDE_USERS
    return {
        "display_name": display_name,
        "manager_owner": manager_owner,
        "department": department,
        "role": role,
        "notes": notes,
        "canonical_user_id": canonical_user_id,
        "exclude": exclude,
    }


def resolve_owner_profile(owner_name):
    owner_name = normalize_management_filter(owner_name)
    if not owner_name:
        owner_name = "unassigned"
    owners = load_manager_owners()
    profile = dict(owners.get(_normalize_identity_key(owner_name), {}))
    display_name = str(profile.get("display_name") or profile.get("name") or owner_name).strip() or owner_name
    title = str(profile.get("title") or profile.get("role") or "").strip()
    department = str(profile.get("department") or "").strip()
    escalation_to = str(profile.get("escalation_to") or profile.get("escalate_to") or "").strip()
    contact = str(profile.get("contact") or profile.get("telegram") or profile.get("email") or "").strip()
    notes = str(profile.get("notes") or "").strip()
    return {
        "owner_name": owner_name,
        "display_name": display_name,
        "title": title,
        "department": department,
        "escalation_to": escalation_to,
        "contact": contact,
        "notes": notes,
    }


def clamp_seconds(value, fallback=DEFAULT_SAMPLE_SECONDS):
    try:
        seconds = float(value)
    except Exception:
        seconds = float(fallback)
    if seconds <= 0:
        seconds = float(fallback)
    return min(seconds, MAX_SAMPLE_SECONDS)


def resolve_host(request_host=None):
    host = (request_host or DEFAULT_HOST).strip()
    if not host:
        host = DEFAULT_HOST
    return host


def get_sessions_bucket_id(host):
    return f"aw-worktime-sessions_{resolve_host(host)}"


def get_management_build_lock(host, report_date):
    key = (resolve_host(host), report_date.isoformat())
    with _MANAGEMENT_BUILD_LOCKS_LOCK:
        lock = _MANAGEMENT_BUILD_LOCKS.get(key)
        if lock is None:
            lock = threading.Lock()
            _MANAGEMENT_BUILD_LOCKS[key] = lock
        return lock


def resolve_report_date(day=None, date_text=None):
    now_local = datetime.now(REPORT_TZ)
    if date_text:
        return datetime.strptime(date_text, "%Y-%m-%d").date()
    if day == "yesterday":
        return (now_local - timedelta(days=1)).date()
    return now_local.date()


def get_report_bounds(report_date):
    start_local = datetime(report_date.year, report_date.month, report_date.day, tzinfo=REPORT_TZ)
    end_local = start_local + timedelta(days=1) - timedelta(seconds=1)
    start = start_local.astimezone(timezone.utc)
    end = end_local.astimezone(timezone.utc)
    end_exclusive = end + timedelta(seconds=1)
    return {
        "start_local": start_local,
        "end_local": end_local,
        "start": start,
        "end": end,
        "end_exclusive": end_exclusive,
    }


def get_workday_bounds(report_date):
    start_local = datetime(report_date.year, report_date.month, report_date.day, WORKDAY_START_HOUR, 0, 0, tzinfo=REPORT_TZ)
    end_local = datetime(report_date.year, report_date.month, report_date.day, WORKDAY_END_HOUR, 0, 0, tzinfo=REPORT_TZ)
    if end_local <= start_local:
        end_local = start_local + timedelta(hours=8)
    return {
        "start_local": start_local,
        "end_local": end_local,
        "duration_seconds": int((end_local - start_local).total_seconds()),
    }


def _is_machine_user(user: str):
    u = (user or "").strip().lower()
    return u.endswith("$") or u in {"system", "localservice", "networkservice"}


def _is_active_sample(data: dict):
    state = str(data.get("state") or "").strip().lower()
    if isinstance(data.get("active"), bool) and data.get("active"):
        return True
    if ("актив" in state) or (state == "active"):
        return True
    if state == "unknown":
        try:
            sid = int(data.get("sessionId"))
        except Exception:
            sid = -1
        user = str(data.get("username") or "").strip()
        session_name = str(data.get("sessionName") or "").strip().lower()
        if sid > 0 and user and (not _is_machine_user(user)) and (session_name.startswith("rdp-") or session_name == "console"):
            return True
    return False


def _normalize_user_id(data, host, username):
    user_id = str(data.get("userId") or "").strip()
    if user_id:
        left, sep, right = user_id.partition("\\")
        if sep and right:
            return f"{resolve_host(host)}\\{right}"
        return user_id
    return f"{resolve_host(host)}\\{username}"


def _event_sample_seconds(event, next_same_session_ts=None):
    data = event.get("data") or {}
    for key in ("sampleSeconds", "pollSeconds"):
        value = data.get(key)
        try:
            if float(value) > 0:
                return clamp_seconds(value)
        except Exception:
            pass
    try:
        duration = float(event.get("duration") or 0.0)
    except Exception:
        duration = 0.0
    if duration > 0:
        return clamp_seconds(duration)
    if next_same_session_ts is not None:
        delta = (next_same_session_ts - event["_ts"]).total_seconds()
        if delta > 0:
            return clamp_seconds(delta)
    return clamp_seconds(DEFAULT_SAMPLE_SECONDS)


def _merge_intervals(intervals):
    if not intervals:
        return []
    ordered = sorted(intervals, key=lambda item: item[0])
    merged = [ordered[0]]
    for start, end in ordered[1:]:
        last_start, last_end = merged[-1]
        if start <= last_end:
            if end > last_end:
                merged[-1] = (last_start, end)
            continue
        merged.append((start, end))
    return merged


def _overlap_interval(left, right):
    start = max(left[0], right[0])
    end = min(left[1], right[1])
    if end <= start:
        return None
    return start, end


def _interval_contains(interval, ts):
    return interval[0] <= ts < interval[1]


def _event_timestamp(event):
    try:
        return pts(event.get("timestamp"))
    except Exception:
        return None


def _event_duration_seconds(event, default_seconds=DEFAULT_SAMPLE_SECONDS):
    try:
        duration = float(event.get("duration") or 0.0)
    except Exception:
        duration = 0.0
    if duration <= 0:
        duration = default_seconds
    return max(1.0, min(float(TRUE_ACTIVE_MAX_EVENT_SECONDS), duration))


def _normalize_app_name(app, title=""):
    app_raw = str(app or "").strip()
    app_l = app_raw.lower()
    title_raw = str(title or "").strip()
    if app_l.startswith(("1cv8", "1cestart")):
        return "1С"
    if app_l in {"chrome.exe", "google chrome"} or "google chrome" in title_raw.lower():
        return "Chrome"
    if app_l in {"msedge.exe", "microsoft edge"} or "microsoft edge" in title_raw.lower():
        return "Edge"
    if app_l in {"browser.exe", "browser"} or "яндекс" in title_raw.lower():
        return "Яндекс Браузер"
    if app_l in {"excel.exe"}:
        return "Excel"
    if app_l in {"winword.exe"}:
        return "Word"
    if app_l in {"powerpnt.exe"}:
        return "PowerPoint"
    if app_l in {"outlook.exe"}:
        return "Outlook"
    if app_l in {"explorer.exe"}:
        return "Проводник"
    if app_l in {"totalcmd.exe", "totalcmd64.exe"}:
        return "Total Commander"
    if app_l in {"cmd.exe"}:
        return "Command Prompt"
    if app_l in {"powershell.exe", "pwsh.exe"}:
        return "PowerShell"
    if app_l in {"acrord32.exe", "acrobat.exe"}:
        return "Adobe Acrobat Reader"
    if app_l in {"windowsterminal.exe", "windowsterminal"}:
        return "Windows Terminal"
    if app_raw:
        return app_raw[:-4] if app_l.endswith(".exe") else app_raw
    if title_raw:
        return title_raw
    return "Неизвестное приложение"


def _event_context(event):
    data = event.get("data") or {}
    for key in ("title", "url", "path", "filePath", "targetPath", "windowTitle", "foregroundTitle", "signalType"):
        value = str(data.get(key) or "").strip()
        if value:
            return value
    return "активность"


def _is_not_afk_event(event):
    data = event.get("data") or {}
    status = str(data.get("status") or data.get("state") or "").strip().lower()
    return status in {"not-afk", "not_afk", "active", "активно"}


def _is_real_evidence_event(event):
    data = event.get("data") or {}
    signal_type = str(data.get("signalType") or data.get("type") or "").strip().lower()
    if signal_type in {"collector_health", "self_test", "heartbeat", "health"}:
        return False
    if data.get("url") or data.get("title") or data.get("path") or data.get("filePath") or data.get("targetPath"):
        return True
    if signal_type:
        return True
    return False


def _events_for_bounds(events, start, end):
    result = []
    for event in events:
        ts = _event_timestamp(event)
        if ts is None or ts < start or ts > end:
            continue
        result.append((ts, event))
    result.sort(key=lambda item: item[0])
    return result


def _build_window_intervals(window_events, start, end):
    intervals = []
    previous_key = None
    for ts, event in _events_for_bounds(window_events, start, end):
        data = event.get("data") or {}
        app = str(data.get("app") or data.get("process") or data.get("processName") or "").strip()
        title = str(data.get("title") or data.get("windowTitle") or "").strip()
        if not app and not title:
            continue
        duration = _event_duration_seconds(event, default_seconds=DEFAULT_SAMPLE_SECONDS)
        interval = (max(ts, start), min(ts + timedelta(seconds=duration), end + timedelta(seconds=1)))
        if interval[1] <= interval[0]:
            continue
        app_name = _normalize_app_name(app, title)
        current_key = (app_name, title)
        title_changed = previous_key is not None and current_key != previous_key
        previous_key = current_key
        intervals.append(
            {
                "app": app_name,
                "raw_app": app,
                "title": title,
                "start": interval[0],
                "end": interval[1],
                "title_changed": title_changed,
                "timestamp": ts,
            }
        )
    return intervals


def _build_not_afk_intervals(afk_events, start, end):
    intervals = []
    for ts, event in _events_for_bounds(afk_events, start, end):
        if not _is_not_afk_event(event):
            continue
        duration = _event_duration_seconds(event, default_seconds=5)
        interval = (max(ts, start), min(ts + timedelta(seconds=duration), end + timedelta(seconds=1)))
        if interval[1] > interval[0]:
            intervals.append(interval)
    return _merge_intervals(intervals)


def _find_window_at(window_intervals, ts):
    for item in window_intervals:
        if item["start"] <= ts < item["end"]:
            return item
    return None


def _add_app_evidence(evidence_by_app, app, ts, context):
    evidence_by_app.setdefault(app, []).append((ts, str(context or "").strip() or "активность"))


def build_true_active_apps_from_events(window_events, afk_events, evidence_events_by_bucket, start, end):
    window_intervals = _build_window_intervals(window_events, start, end)
    not_afk_intervals = _build_not_afk_intervals(afk_events, start, end)
    evidence_by_app = {}

    for item in window_intervals:
        if item["title_changed"]:
            _add_app_evidence(evidence_by_app, item["app"], item["timestamp"], item["title"] or item["raw_app"])

    for events in evidence_events_by_bucket.values():
        for ts, event in _events_for_bounds(events, start, end):
            if not _is_real_evidence_event(event):
                continue
            window = _find_window_at(window_intervals, ts)
            if window is None:
                continue
            _add_app_evidence(evidence_by_app, window["app"], ts, _event_context(event))

    rows = []
    evidence_delta = timedelta(seconds=TRUE_ACTIVE_EVIDENCE_WINDOW_SECONDS)
    for app in sorted({item["app"] for item in window_intervals} | set(evidence_by_app)):
        app_evidence = sorted(evidence_by_app.get(app, []), key=lambda item: item[0])
        if not app_evidence:
            continue
        evidence_windows = _merge_intervals([(ts - evidence_delta, ts + evidence_delta) for ts, _context in app_evidence])
        proved_intervals = []
        for window in [item for item in window_intervals if item["app"] == app]:
            base = (window["start"], window["end"])
            for afk_interval in not_afk_intervals:
                active_overlap = _overlap_interval(base, afk_interval)
                if active_overlap is None:
                    continue
                for evidence_interval in evidence_windows:
                    proved = _overlap_interval(active_overlap, evidence_interval)
                    if proved is not None:
                        proved_intervals.append(proved)
        proved_intervals = _merge_intervals(proved_intervals)
        proved_seconds = int(sum((right - left).total_seconds() for left, right in proved_intervals))
        if proved_seconds <= 0:
            continue
        last_ts, last_context = app_evidence[-1]
        rows.append(
            {
                "application": app,
                "proved_work_seconds": proved_seconds,
                "proved_work_hhmm": hhmm(proved_seconds),
                "proved_work_human": human_duration_ru(proved_seconds),
                "last_action_utc": to_iso_utc(last_ts),
                "last_action_local": last_ts.astimezone(REPORT_TZ).strftime("%H:%M"),
                "last_action": last_context,
                "evidence_events": len(app_evidence),
            }
        )
    rows.sort(key=lambda item: (-item["proved_work_seconds"], item["application"].lower()))
    return rows


def build_true_active_apps(host, report_date):
    bounds = get_report_bounds(report_date)
    host = resolve_host(host)
    window_events = fetch_bucket_events(f"aw-watcher-window_{host}", host) or fetch_bucket_events(f"aw-rdp-window_{host}", host)
    afk_events = fetch_bucket_events(f"aw-watcher-afk_{host}", host) or fetch_bucket_events(f"aw-rdp-afk_{host}", host)
    evidence_events_by_bucket = {}
    for bucket_id in (
        f"aw-file-operations_{host}",
        f"aw-dlp-endpoint-signals_{host}",
        f"aw-watcher-web-chrome_{host}",
        f"aw-watcher-web-edge_{host}",
        f"aw-detmir-web-category_{host}",
    ):
        evidence_events_by_bucket[bucket_id] = fetch_bucket_events(bucket_id, host)
    return build_true_active_apps_from_events(
        window_events,
        afk_events,
        evidence_events_by_bucket,
        bounds["start"],
        bounds["end"],
    )


def _collect_user_rows(events, start, end, host):
    end_exclusive = end + timedelta(seconds=1)
    by_user = {}
    by_identity = {}

    for event in events:
        ts = pts(event.get("timestamp"))
        if ts < start or ts > end:
            continue
        data = event.get("data") or {}
        username = str(data.get("username") or "").strip()
        if not username:
            continue
        session_id = str(data.get("sessionId") or "").strip() or "unknown"
        event_copy = {
            "_ts": ts,
            "data": data,
            "duration": event.get("duration"),
        }
        by_identity.setdefault((username, session_id), []).append(event_copy)

    for (username, session_id), samples in by_identity.items():
        ordered = sorted(samples, key=lambda item: item["_ts"])
        for idx, sample in enumerate(ordered):
            data = sample["data"]
            active = _is_active_sample(data)
            next_ts = ordered[idx + 1]["_ts"] if idx + 1 < len(ordered) else None
            sample_seconds = _event_sample_seconds(sample, next_ts)
            row = by_user.setdefault(
                username,
                {
                    "user": username,
                    "user_id": _normalize_user_id(data, host, username),
                    "samples_count": 0,
                    "active_samples": 0,
                    "session_ids": set(),
                    "intervals": [],
                },
            )
            row["samples_count"] += 1
            row["session_ids"].add(session_id)
            if active:
                row["active_samples"] += 1
                interval_start = sample["_ts"]
                interval_end = min(sample["_ts"] + timedelta(seconds=sample_seconds), end_exclusive)
                if interval_end > interval_start:
                    row["intervals"].append((interval_start, interval_end))
    return by_user


def aggregate_rows(events, start, end, host):
    by_user = _collect_user_rows(events, start, end, host)
    return _build_rows_from_user_map(by_user, start, end, include_intervals=False)


def aggregate_rows_with_intervals(events, start, end, host):
    by_user = _collect_user_rows(events, start, end, host)
    return _build_rows_from_user_map(by_user, start, end, include_intervals=True)


def _build_rows_from_user_map(by_user, start, end, include_intervals):
    rows = []
    full_range = int((end - start).total_seconds()) + 1
    for username in sorted(by_user):
        row = by_user[username]
        merged = _merge_intervals(row["intervals"])
        active_seconds = int(sum((end_dt - start_dt).total_seconds() for start_dt, end_dt in merged))
        active_seconds = min(active_seconds, full_range)
        first_activity = to_iso_utc(merged[0][0]) if merged else ""
        last_activity = to_iso_utc(merged[-1][1]) if merged else ""
        rows.append(
            {
                "user": row["user"],
                "user_id": row["user_id"],
                "active_seconds": active_seconds,
                "active_hhmm": hhmm(active_seconds),
                "first_activity": first_activity,
                "last_activity": last_activity,
                "idle_seconds": max(0, full_range - active_seconds),
                "sessions_count": len(row["session_ids"]),
                "samples_count": row["samples_count"],
                "active_samples": row["active_samples"],
            }
        )
        if include_intervals:
            rows[-1]["_intervals"] = merged
    return rows


def aggregate_hourly_rows(events, start, end, host):
    by_user = _collect_user_rows(events, start, end, host)
    rows = []
    for username in sorted(by_user):
        row = by_user[username]
        merged = _merge_intervals(row["intervals"])
        per_bucket = {}
        for interval_start, interval_end in merged:
            cursor = interval_start
            while cursor < interval_end:
                bucket_local = cursor.astimezone(REPORT_TZ).replace(minute=0, second=0, microsecond=0)
                bucket_start = bucket_local.astimezone(timezone.utc)
                bucket_end = (bucket_local + timedelta(hours=1)).astimezone(timezone.utc)
                overlap_start = max(interval_start, bucket_start)
                overlap_end = min(interval_end, bucket_end)
                if overlap_end > overlap_start:
                    key = bucket_start
                    per_bucket[key] = per_bucket.get(key, 0) + int((overlap_end - overlap_start).total_seconds())
                cursor = bucket_end

        for bucket_start in sorted(per_bucket):
            active_seconds = per_bucket[bucket_start]
            if active_seconds <= 0:
                continue
            bucket_local = bucket_start.astimezone(REPORT_TZ)
            rows.append(
                {
                    "user": row["user"],
                    "user_id": row["user_id"],
                    "bucket_start_utc": to_iso_utc(bucket_start),
                    "bucket_start_local": bucket_local.isoformat(),
                    "report_date": bucket_local.date().isoformat(),
                    "hour_local": bucket_local.strftime("%H:00"),
                    "active_seconds": active_seconds,
                    "active_hhmm": hhmm(active_seconds),
                }
            )
    return rows


def fetch_events_for_date(host, report_date):
    bounds = get_report_bounds(report_date)
    bucket_id = get_sessions_bucket_id(host)
    events = fetch_bucket_events(bucket_id, host)
    return bounds, events


def fetch_bucket_events(bucket_id, host):
    now = now_utc()
    if EVENTS_CACHE_TTL_SECONDS > 0:
        with _EVENTS_CACHE_LOCK:
            cached = _EVENTS_CACHE.get(bucket_id)
            if cached is not None and age_seconds(cached["stored_at"], now=now) <= EVENTS_CACHE_TTL_SECONDS:
                return cached["events"]
    try:
        get(f"{AW}/buckets/{bucket_id}")
    except Exception:
        log_warning(f"bucket lookup failed for host={host} bucket={bucket_id} aw_base={AW}")
        return []
    try:
        events = get(f"{AW}/buckets/{bucket_id}/events?limit={WORKTIME_EVENTS_LIMIT}")
    except Exception:
        log_warning(f"events fetch failed for host={host} bucket={bucket_id} aw_base={AW}")
        return []
    if EVENTS_CACHE_TTL_SECONDS > 0:
        with _EVENTS_CACHE_LOCK:
            _EVENTS_CACHE[bucket_id] = {"stored_at": now, "events": events}
    return events


def build_report_summary(rows):
    if not rows:
        return {
            "users_count": 0,
            "total_active_seconds": 0,
            "total_active_hhmm": "00:00",
            "first_activity": "",
            "last_activity": "",
            "top_user": "",
            "top_user_active_hhmm": "00:00",
        }

    total_active_seconds = sum(int(row.get("active_seconds", 0) or 0) for row in rows)
    first_values = [row.get("first_activity") for row in rows if row.get("first_activity")]
    last_values = [row.get("last_activity") for row in rows if row.get("last_activity")]
    top_row = max(rows, key=lambda row: int(row.get("active_seconds", 0) or 0))
    return {
        "users_count": len(rows),
        "total_active_seconds": total_active_seconds,
        "total_active_hhmm": hhmm(total_active_seconds),
        "first_activity": min(first_values) if first_values else "",
        "last_activity": max(last_values) if last_values else "",
        "top_user": top_row.get("user", ""),
        "top_user_active_hhmm": top_row.get("active_hhmm", "00:00"),
    }


def latest_bucket_event(bucket_id):
    try:
        events = get(f"{AW}/buckets/{bucket_id}/events?limit=20")
    except Exception:
        return None
    if not isinstance(events, list) or not events:
        return None
    valid = [item for item in events if isinstance(item, dict)]
    if not valid:
        return None
    valid.sort(key=lambda item: item.get("timestamp") or "", reverse=True)
    return valid[0]


def _priority_rank(priority):
    return {"critical": 0, "high": 1, "medium": 2, "low": 3}.get(priority, 9)


def _clamp_pct(value):
    return round(min(100.0, max(0.0, float(value))), 2)


def _action(action_id, priority, owner, deadline_hint, reason, recommended_action, *, user_id="", evidence=None):
    return {
        "action_id": action_id,
        "priority": priority,
        "owner": owner,
        "user_id": user_id,
        "deadline_hint": deadline_hint,
        "reason": reason,
        "recommended_action": recommended_action,
        "evidence": evidence or {},
    }


def build_executive_summary(summary, actions, sources):
    critical = [action for action in actions if action.get("priority") == "critical"]
    high = [action for action in actions if action.get("priority") == "high"]
    stale_sources = [source for source in sources if source.get("status") != "ok"]
    if critical:
        portfolio_state = "critical"
        headline = f"Есть {len(critical)} критичных вопроса, требующих решения сегодня."
    elif high:
        portfolio_state = "attention"
        headline = f"Критичных провалов нет, но есть {len(high)} вопроса повышенного внимания."
    elif summary.get("portfolio_coverage_pct", 0.0) < MANAGER_TARGET_COVERAGE_PCT:
        portfolio_state = "attention"
        headline = "Покрытие ниже целевого порога, но явных критичных кейсов не найдено."
    else:
        portfolio_state = "stable"
        headline = "Критичных отклонений не найдено, рабочий день идёт в пределах нормы."

    message_parts = [
        f"Активны {summary.get('active_users', 0)} из {summary.get('users_count', 0)} сотрудников.",
        f"Покрытие рабочего окна {summary.get('portfolio_coverage_pct', 0.0)}%.",
    ]
    if stale_sources:
        message_parts.append(f"Есть {len(stale_sources)} проблем(ы) со свежестью источников.")
    message = " ".join(message_parts)

    focus_items = []
    for action in actions[:5]:
        focus_items.append(
            {
                "priority": action["priority"],
                "owner": action["owner"],
                "title": action["action_id"],
                "reason": action["reason"],
                "recommended_action": action["recommended_action"],
            }
        )
    stale_items = []
    for source in stale_sources[:3]:
        stale_items.append(
            {
                "source_id": source["source_id"],
                "label": source["label"],
                "status": source["status_label"],
                "summary": source["summary"],
            }
        )
    return {
        "portfolio_state": portfolio_state,
        "headline": headline,
        "message": message,
        "focus_items": focus_items,
        "stale_sources": stale_items,
    }


def _empty_rollup(name):
    return {
        "name": name or "unassigned",
        "users_count": 0,
        "active_users": 0,
        "inactive_users": 0,
        "below_target_users": 0,
        "workday_total_active_seconds": 0,
        "workday_total_active_hhmm": "00:00",
        "portfolio_coverage_pct": 0.0,
        "actions_count": 0,
        "critical_actions_count": 0,
        "high_actions_count": 0,
        "medium_actions_count": 0,
        "low_actions_count": 0,
        "users": [],
    }


def normalize_management_filter(value):
    if value is None:
        return ""
    return " ".join(str(value).split()).strip()


def _filter_key(value):
    return normalize_management_filter(value).casefold()


def _matches_management_filters(alias, owner_filter="", department_filter=""):
    if owner_filter and _filter_key(alias.get("manager_owner")) != _filter_key(owner_filter):
        return False
    if department_filter and _filter_key(alias.get("department")) != _filter_key(department_filter):
        return False
    return True


def _action_department(action):
    evidence = action.get("evidence") or {}
    department = normalize_management_filter(evidence.get("department"))
    if department:
        return department
    if normalize_management_filter(action.get("owner")) == "ops":
        return "Инфраструктура"
    return ""


def filter_management_actions(actions, rows, owner_filter="", department_filter=""):
    owner_filter = normalize_management_filter(owner_filter)
    department_filter = normalize_management_filter(department_filter)
    if not owner_filter and not department_filter:
        return list(actions)
    row_user_ids = {
        normalize_management_filter(row.get("canonical_user_id") or row.get("user_id"))
        for row in rows
        if normalize_management_filter(row.get("canonical_user_id") or row.get("user_id"))
    }
    filtered = []
    for action in actions:
        if owner_filter and _filter_key(action.get("owner")) != _filter_key(owner_filter):
            continue
        if department_filter:
            action_department = _action_department(action)
            if action_department:
                if _filter_key(action_department) != _filter_key(department_filter):
                    continue
            else:
                user_id = normalize_management_filter(action.get("user_id"))
                if not user_id or user_id not in row_user_ids:
                    continue
        filtered.append(action)
    return filtered


def _row_matches_payload_filters(row, owner_filter="", department_filter=""):
    if owner_filter and _filter_key(row.get("manager_owner")) != _filter_key(owner_filter):
        return False
    if department_filter and _filter_key(row.get("department")) != _filter_key(department_filter):
        return False
    return True


def summarize_management_rows(rows, actions, workday):
    expected_seconds_per_user = int(workday.get("expected_seconds_per_user", 0) or 0)
    workday_total_active_seconds = sum(int(row.get("workday_active_seconds", 0) or 0) for row in rows)
    calendar_total_active_seconds = sum(int(row.get("calendar_active_seconds", 0) or 0) for row in rows)
    active_users = sum(1 for row in rows if row.get("status") != "inactive")
    inactive_users = sum(1 for row in rows if row.get("status") == "inactive")
    below_target_users = sum(1 for row in rows if row.get("status") == "below_target")
    on_target_users = sum(1 for row in rows if row.get("status") == "ok")
    workday_first_values = sorted(str(row.get("workday_first_activity_local") or "") for row in rows if row.get("workday_first_activity_local"))
    workday_last_values = sorted(str(row.get("workday_last_activity_local") or "") for row in rows if row.get("workday_last_activity_local"))
    calendar_first_values = sorted(str(row.get("first_activity_local") or "") for row in rows if row.get("first_activity_local"))
    calendar_last_values = sorted(str(row.get("last_activity_local") or "") for row in rows if row.get("last_activity_local"))
    top_row = max(rows, key=lambda row: int(row.get("workday_active_seconds", 0) or 0), default=None)
    portfolio_coverage_pct = _clamp_pct((workday_total_active_seconds / (expected_seconds_per_user * len(rows))) * 100.0) if rows and expected_seconds_per_user > 0 else 0.0
    return {
        "users_count": len(rows),
        "active_users": active_users,
        "inactive_users": inactive_users,
        "on_target_users": on_target_users,
        "below_target_users": below_target_users,
        "portfolio_coverage_pct": portfolio_coverage_pct,
        "actions_count": len(actions),
        "critical_actions_count": sum(1 for action in actions if action.get("priority") == "critical"),
        "high_actions_count": sum(1 for action in actions if action.get("priority") == "high"),
        "calendar_total_active_seconds": calendar_total_active_seconds,
        "calendar_total_active_hhmm": hhmm(calendar_total_active_seconds),
        "calendar_first_activity": calendar_first_values[0] if calendar_first_values else "",
        "calendar_last_activity": calendar_last_values[-1] if calendar_last_values else "",
        "workday_total_active_seconds": workday_total_active_seconds,
        "workday_total_active_hhmm": hhmm(workday_total_active_seconds),
        "workday_first_activity": workday_first_values[0] if workday_first_values else "",
        "workday_last_activity": workday_last_values[-1] if workday_last_values else "",
        "total_active_seconds": workday_total_active_seconds,
        "total_active_hhmm": hhmm(workday_total_active_seconds),
        "first_activity": workday_first_values[0] if workday_first_values else "",
        "last_activity": workday_last_values[-1] if workday_last_values else "",
        "top_user": str((top_row or {}).get("user") or ""),
        "top_user_active_hhmm": hhmm(int((top_row or {}).get("workday_active_seconds", 0) or 0)),
    }


def apply_management_filters_to_payload(payload, owner_filter="", department_filter="", include_sources=True, include_source_actions=True):
    owner_filter = normalize_management_filter(owner_filter)
    department_filter = normalize_management_filter(department_filter)
    filtered_rows = [
        dict(row)
        for row in payload.get("rows", [])
        if _row_matches_payload_filters(row, owner_filter=owner_filter, department_filter=department_filter)
    ]
    base_actions = list(payload.get("actions", []))
    if not include_source_actions:
        base_actions = [action for action in base_actions if action.get("action_id") != "source_freshness_review"]
    filtered_actions = filter_management_actions(base_actions, filtered_rows, owner_filter=owner_filter, department_filter=department_filter)
    filtered_payload = dict(payload)
    filtered_payload["filters"] = {
        "owner": owner_filter,
        "department": department_filter,
    }
    filtered_payload["rows"] = filtered_rows
    filtered_payload["actions"] = filtered_actions
    filtered_payload["summary"] = summarize_management_rows(filtered_rows, filtered_actions, payload.get("workday") or {})
    filtered_payload["owner_rollups"] = build_owner_rollups(filtered_rows, filtered_actions)
    filtered_payload["department_rollups"] = build_department_rollups(filtered_rows, filtered_actions)
    filtered_payload["owner_roster"] = build_owner_roster(filtered_rows, filtered_actions)
    if include_sources:
        filtered_payload["sources"] = list(payload.get("sources", []))
    else:
        filtered_payload["sources"] = []
    filtered_payload["executive"] = build_executive_summary(filtered_payload["summary"], filtered_actions, filtered_payload["sources"])
    return filtered_payload


def build_owner_rollups(rows, actions):
    groups = {}
    for row in rows:
        owner = str(row.get("manager_owner") or row.get("user") or "unassigned").strip() or "unassigned"
        group = groups.setdefault(owner, _empty_rollup(owner))
        group["users_count"] += 1
        if row.get("status") == "inactive":
            group["inactive_users"] += 1
        else:
            group["active_users"] += 1
        if row.get("status") == "below_target":
            group["below_target_users"] += 1
        group["workday_total_active_seconds"] += int(row.get("workday_active_seconds", 0) or 0)
        group["users"].append(row.get("user", "unknown"))

    for action in actions:
        owner = str(action.get("owner") or "unassigned").strip() or "unassigned"
        group = groups.setdefault(owner, _empty_rollup(owner))
        group["actions_count"] += 1
        prio = str(action.get("priority") or "").lower()
        if prio == "critical":
            group["critical_actions_count"] += 1
        elif prio == "high":
            group["high_actions_count"] += 1
        elif prio == "medium":
            group["medium_actions_count"] += 1
        elif prio == "low":
            group["low_actions_count"] += 1

    for group in groups.values():
        expected = group["users_count"] * max(1, WORKDAY_END_HOUR - WORKDAY_START_HOUR) * 3600
        group["workday_total_active_hhmm"] = hhmm(group["workday_total_active_seconds"])
        group["portfolio_coverage_pct"] = _clamp_pct((group["workday_total_active_seconds"] / expected) * 100.0) if expected > 0 else 0.0
        group["users"] = sorted(set(str(user or "").strip() for user in group["users"] if str(user or "").strip()))

    return sorted(
        groups.values(),
        key=lambda item: (
            -item["critical_actions_count"],
            -item["high_actions_count"],
            -item["inactive_users"],
            item["name"].lower(),
        ),
    )


def build_department_rollups(rows, actions):
    groups = {}
    for row in rows:
        department = str(row.get("department") or "Без подразделения").strip() or "Без подразделения"
        group = groups.setdefault(department, _empty_rollup(department))
        group["users_count"] += 1
        if row.get("status") == "inactive":
            group["inactive_users"] += 1
        else:
            group["active_users"] += 1
        if row.get("status") == "below_target":
            group["below_target_users"] += 1
        group["workday_total_active_seconds"] += int(row.get("workday_active_seconds", 0) or 0)
        group["users"].append(row.get("user", "unknown"))

    for action in actions:
        evidence = action.get("evidence") or {}
        department = str(evidence.get("department") or ("Инфраструктура" if str(action.get("owner") or "").strip().lower() == "ops" else "Без подразделения")).strip()
        group = groups.setdefault(department, _empty_rollup(department))
        group["actions_count"] += 1
        prio = str(action.get("priority") or "").lower()
        if prio == "critical":
            group["critical_actions_count"] += 1
        elif prio == "high":
            group["high_actions_count"] += 1
        elif prio == "medium":
            group["medium_actions_count"] += 1
        elif prio == "low":
            group["low_actions_count"] += 1

    for group in groups.values():
        expected = group["users_count"] * max(1, WORKDAY_END_HOUR - WORKDAY_START_HOUR) * 3600
        group["workday_total_active_hhmm"] = hhmm(group["workday_total_active_seconds"])
        group["portfolio_coverage_pct"] = _clamp_pct((group["workday_total_active_seconds"] / expected) * 100.0) if expected > 0 else 0.0
        group["users"] = sorted(set(str(user or "").strip() for user in group["users"] if str(user or "").strip()))

    return sorted(
        groups.values(),
        key=lambda item: (
            -item["critical_actions_count"],
            -item["high_actions_count"],
            -item["inactive_users"],
            item["name"].lower(),
        ),
    )


def build_owner_roster(rows, actions):
    owner_rollups = build_owner_rollups(rows, actions)
    roster = []
    for item in owner_rollups:
        profile = resolve_owner_profile(item["name"])
        roster.append(
            {
                **item,
                "display_name": profile["display_name"],
                "title": profile["title"],
                "department": profile["department"],
                "contact": profile["contact"],
                "escalation_to": profile["escalation_to"],
                "notes": profile["notes"],
            }
        )
    return roster


def _interval_overlap_seconds(intervals, start, end):
    total = 0
    first = None
    last = None
    for interval_start, interval_end in intervals or []:
        overlap_start = max(interval_start, start)
        overlap_end = min(interval_end, end)
        if overlap_end <= overlap_start:
            continue
        seconds = int((overlap_end - overlap_start).total_seconds())
        if seconds <= 0:
            continue
        total += seconds
        if first is None or overlap_start < first:
            first = overlap_start
        if last is None or overlap_end > last:
            last = overlap_end
    return total, first, last


def _source_status_label(status):
    return {
        "ok": "fresh",
        "warn": "stale",
        "fail": "missing",
    }.get(status, status)


def _source_summary(event):
    data = (event or {}).get("data") or {}
    signal_type = str(data.get("signalType") or "").strip()
    if signal_type == "collector_health":
        return (
            f"queue={data.get('queueDepth', 0)} "
            f"failures={data.get('sendFailures', 0)} "
            f"flushed={data.get('eventsFlushed', 0)}"
        )
    if data.get("domain"):
        return f"{data.get('domain')} ({data.get('category', 'uncategorized')})"
    if data.get("eventType"):
        return f"{data.get('eventType')} {data.get('username', '')}".strip()
    if data.get("action"):
        return f"{data.get('action')} {data.get('result', '')}".strip()
    if data.get("title"):
        return str(data.get("title"))[:120]
    if data.get("status"):
        return str(data.get("status"))
    if data.get("app"):
        return str(data.get("app"))
    return ""


def management_cache_path(host, report_date):
    host_slug = safe_slug(host)
    return MANAGER_CACHE_DIR / f"{host_slug}-{report_date.isoformat()}.json"


def load_management_cache(host, report_date):
    if MANAGER_CACHE_TTL_SECONDS <= 0:
        return None
    path = management_cache_path(host, report_date)
    if not path.exists():
        return None
    if report_date >= datetime.now(REPORT_TZ).date():
        age = age_seconds(datetime.fromtimestamp(path.stat().st_mtime, tz=timezone.utc))
        if age is None or age > MANAGER_CACHE_TTL_SECONDS:
            return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return None


def save_management_cache(host, report_date, payload):
    if MANAGER_CACHE_TTL_SECONDS <= 0:
        return
    write_atomic_json(management_cache_path(host, report_date), payload)


def build_source_freshness(host):
    source_specs = [
        {
            "source_id": "worktime_sessions",
            "label": "RDP worktime sessions",
            "bucket_candidates": [f"aw-worktime-sessions_{host}"],
            "max_age_seconds": MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS,
            "required": True,
            "owner": "ops",
        },
        {
            "source_id": "rdp_window",
            "label": "RDP current window",
            "bucket_candidates": [f"aw-rdp-window_{host}"],
            "max_age_seconds": MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS,
            "required": True,
            "owner": "ops",
        },
        {
            "source_id": "rdp_afk",
            "label": "RDP AFK",
            "bucket_candidates": [f"aw-rdp-afk_{host}"],
            "max_age_seconds": MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS,
            "required": True,
            "owner": "ops",
        },
        {
            "source_id": "watcher_window",
            "label": "Local watcher window",
            "bucket_candidates": [f"aw-watcher-window_{host}"],
            "max_age_seconds": MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS,
            "required": True,
            "owner": "ops",
        },
        {
            "source_id": "watcher_afk",
            "label": "Local watcher AFK",
            "bucket_candidates": [f"aw-watcher-afk_{host}"],
            "max_age_seconds": MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS,
            "required": True,
            "owner": "ops",
        },
        {
            "source_id": "file_operations",
            "label": "File operations collector",
            "bucket_candidates": [f"aw-file-operations_{host}"],
            "max_age_seconds": MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS,
            "required": True,
            "owner": "ops",
        },
        {
            "source_id": "web_categories",
            "label": "Browser/web categories",
            "bucket_candidates": [f"aw-detmir-web-category_{host}"],
            "max_age_seconds": MANAGER_WEB_SOURCE_MAX_AGE_SECONDS,
            "required": False,
            "owner": "ops",
        },
        {
            "source_id": "session_events",
            "label": "Windows session events",
            "bucket_candidates": [f"aw-session-events_{host}"],
            "max_age_seconds": MANAGER_SESSION_SOURCE_MAX_AGE_SECONDS,
            "required": False,
            "owner": "ops",
        },
        {
            "source_id": "pve_tasks",
            "label": "PVE task feed",
            "bucket_candidates": ["aw-pve-task-events_pve-detmir"],
            "max_age_seconds": MANAGER_INFRA_SOURCE_MAX_AGE_SECONDS,
            "required": False,
            "owner": "ops",
        },
    ]
    now = now_utc()
    sources = []
    actions = []
    for spec in source_specs:
        matched_bucket = ""
        matched_event = None
        matched_age = None
        for candidate in spec["bucket_candidates"]:
            event = latest_bucket_event(candidate)
            if not event:
                continue
            ts = parse_iso_utc(event.get("timestamp"))
            candidate_age = age_seconds(ts, now=now)
            if matched_event is None or (candidate_age is not None and (matched_age is None or candidate_age < matched_age)):
                matched_bucket = candidate
                matched_event = event
                matched_age = candidate_age
        ts = parse_iso_utc((matched_event or {}).get("timestamp"))
        age = matched_age if matched_event is not None else age_seconds(ts, now=now)
        if matched_event is None:
            status = "fail" if spec["required"] else "warn"
            summary = "bucket missing or empty"
        elif age is None:
            status = "warn"
            summary = "timestamp parse failed"
        elif age > spec["max_age_seconds"]:
            status = "fail" if spec["required"] else "warn"
            summary = f"stale ({age}s)"
        else:
            status = "ok"
            summary = f"fresh ({age}s)"
        detail = {
            "source_id": spec["source_id"],
            "label": spec["label"],
            "status": status,
            "status_label": _source_status_label(status),
            "bucket_id": matched_bucket or spec["bucket_candidates"][0],
            "timestamp": (matched_event or {}).get("timestamp", ""),
            "age_seconds": age,
            "required": spec["required"],
            "max_age_seconds": spec["max_age_seconds"],
            "summary": summary,
            "event_summary": _source_summary(matched_event),
        }
        sources.append(detail)
        if status == "ok":
            continue
        priority = "critical" if spec["required"] else "medium"
        actions.append(
            _action(
                "source_freshness_review",
                priority,
                spec["owner"],
                "today" if spec["required"] else "3d",
                f"Источник '{spec['label']}' в состоянии {detail['status_label']}: {summary}.",
                "Проверить collector/service, причину отставания и подтвердить, что управленческие выводы по данным ещё надёжны.",
                evidence={
                    "source_id": spec["source_id"],
                    "bucket_id": detail["bucket_id"],
                    "age_seconds": age,
                    "required": spec["required"],
                },
            )
        )
    return sources, actions


def _build_management_core(rows, host, report_date, owner_filter="", department_filter=""):
    owner_filter = normalize_management_filter(owner_filter)
    department_filter = normalize_management_filter(department_filter)
    report_bounds = get_report_bounds(report_date)
    workday = get_workday_bounds(report_date)
    now_local = datetime.now(REPORT_TZ)
    is_today = report_date == now_local.date()
    effective_end_local = min(now_local, workday["end_local"]) if is_today else workday["end_local"]
    elapsed_seconds = max(0, int((effective_end_local - workday["start_local"]).total_seconds()))
    if not is_today:
        elapsed_seconds = workday["duration_seconds"]
    expected_seconds_per_user = min(workday["duration_seconds"], max(0, elapsed_seconds))
    target_seconds = int(expected_seconds_per_user * (MANAGER_TARGET_COVERAGE_PCT / 100.0))
    low_seconds = int(expected_seconds_per_user * (MANAGER_LOW_COVERAGE_PCT / 100.0))
    late_start_local = workday["start_local"] + timedelta(minutes=MANAGER_LATE_START_GRACE_MINUTES)
    early_finish_local = workday["end_local"] - timedelta(minutes=MANAGER_EARLY_FINISH_GRACE_MINUTES)

    roster = []
    actions = []
    active_users = 0
    on_target_users = 0
    below_target_users = 0
    workday_total_active_seconds = 0
    workday_first_values = []
    workday_last_values = []
    top_workday_user = ""
    top_workday_seconds = -1
    filtered_rows = []

    for row in rows:
        alias = resolve_user_alias(row.get("user", ""), row.get("user_id", ""), host)
        if alias["exclude"]:
            continue
        if not _matches_management_filters(alias, owner_filter=owner_filter, department_filter=department_filter):
            continue
        filtered_rows.append(row)
        public_row = {key: value for key, value in row.items() if key != "_intervals"}
        calendar_active_seconds = int(row.get("active_seconds", 0) or 0)
        intervals = row.get("_intervals") or []
        workday_active_seconds, workday_first, workday_last = _interval_overlap_seconds(
            intervals,
            workday["start_local"].astimezone(timezone.utc),
            effective_end_local.astimezone(timezone.utc),
        )
        first_activity = parse_iso_utc(row.get("first_activity"))
        last_activity = parse_iso_utc(row.get("last_activity"))
        first_local = first_activity.astimezone(REPORT_TZ) if first_activity else None
        last_local = last_activity.astimezone(REPORT_TZ) if last_activity else None
        workday_first_local = workday_first.astimezone(REPORT_TZ) if workday_first else None
        workday_last_local = workday_last.astimezone(REPORT_TZ) if workday_last else None
        coverage_pct = _clamp_pct((workday_active_seconds / expected_seconds_per_user) * 100.0) if expected_seconds_per_user > 0 else 0.0
        status = "ok"
        if workday_active_seconds <= 0:
            status = "inactive"
        elif workday_active_seconds < target_seconds:
            status = "below_target"
        if workday_active_seconds > 0:
            active_users += 1
            workday_total_active_seconds += workday_active_seconds
            if workday_first_local:
                workday_first_values.append(workday_first_local.isoformat())
            if workday_last_local:
                workday_last_values.append(workday_last_local.isoformat())
        if workday_active_seconds >= target_seconds and workday_active_seconds > 0:
            on_target_users += 1
        elif workday_active_seconds > 0:
            below_target_users += 1
        if workday_active_seconds > top_workday_seconds:
            top_workday_seconds = workday_active_seconds
            top_workday_user = alias["display_name"]

        roster.append(
            {
                **public_row,
                "user": alias["display_name"],
                "user_original": row.get("user", ""),
                "manager_owner": alias["manager_owner"],
                "department": alias["department"],
                "role": alias["role"],
                "notes": alias["notes"],
                "canonical_user_id": alias["canonical_user_id"] or row.get("user_id", ""),
                "calendar_active_seconds": calendar_active_seconds,
                "calendar_active_hhmm": row.get("active_hhmm", "00:00"),
                "workday_active_seconds": workday_active_seconds,
                "workday_active_hhmm": hhmm(workday_active_seconds),
                "coverage_pct": coverage_pct,
                "status": status,
                "first_activity_local": first_local.isoformat() if first_local else "",
                "last_activity_local": last_local.isoformat() if last_local else "",
                "workday_first_activity_local": workday_first_local.isoformat() if workday_first_local else "",
                "workday_last_activity_local": workday_last_local.isoformat() if workday_last_local else "",
            }
        )

        owner = alias["display_name"]
        user_id = alias["canonical_user_id"] or row.get("user_id", "")
        evidence = {
            "calendar_active_hhmm": row.get("active_hhmm", "00:00"),
            "workday_active_hhmm": hhmm(workday_active_seconds),
            "coverage_pct": coverage_pct,
            "first_activity": row.get("first_activity", ""),
            "last_activity": row.get("last_activity", ""),
            "sessions_count": row.get("sessions_count", 0),
            "manager_owner": alias["manager_owner"],
            "department": alias["department"],
            "role": alias["role"],
        }
        if workday_active_seconds <= 0:
            actions.append(
                _action(
                    "missing_activity",
                    "critical",
                    alias["manager_owner"],
                    "today",
                    f"За {report_date.isoformat()} у сотрудника {owner} нет подтверждённой активности в рабочем окне RDP.",
                    f"Проверить сотрудника {owner}: работал ли он в рабочее время, была ли потеря сбора данных или отсутствие входа в систему.",
                    user_id=user_id,
                    evidence=evidence,
                )
            )
            continue
        if expected_seconds_per_user > 0 and workday_active_seconds < low_seconds:
            actions.append(
                _action(
                    "low_activity_review",
                    "high",
                    alias["manager_owner"],
                    "24h",
                    f"У сотрудника {owner} активное время в рабочем окне {hhmm(workday_active_seconds)} ниже {MANAGER_LOW_COVERAGE_PCT}% от ожидаемого окна.",
                    f"Проверить загрузку сотрудника {owner}, задачи и фактическое присутствие в рабочем процессе.",
                    user_id=user_id,
                    evidence=evidence,
                )
            )
        elif expected_seconds_per_user > 0 and workday_active_seconds < target_seconds:
            actions.append(
                _action(
                    "target_gap_review",
                    "medium",
                    alias["manager_owner"],
                    "24h",
                    f"У сотрудника {owner} активное время в рабочем окне {hhmm(workday_active_seconds)} ниже управленческого целевого порога {MANAGER_TARGET_COVERAGE_PCT}%.",
                    f"Уточнить причину отклонения по сотруднику {owner} и подтвердить план работ.",
                    user_id=user_id,
                    evidence=evidence,
                )
            )
        if workday_first_local and workday_first_local > late_start_local:
            actions.append(
                _action(
                    "late_start_review",
                    "medium",
                    alias["manager_owner"],
                    "24h",
                    f"У сотрудника {owner} первая активность в рабочем окне зафиксирована поздно: {workday_first_local.strftime('%H:%M')}.",
                    f"Проверить причину позднего старта сотрудника {owner} и подтвердить, что это не проблема доступа или дисциплины.",
                    user_id=user_id,
                    evidence=evidence,
                )
            )
        if (not is_today) and workday_last_local and workday_last_local < early_finish_local:
            actions.append(
                _action(
                    "early_finish_review",
                    "medium",
                    alias["manager_owner"],
                    "24h",
                    f"У сотрудника {owner} последняя активность в рабочем окне завершилась рано: {workday_last_local.strftime('%H:%M')}.",
                    f"Проверить, было ли досрочное завершение рабочего дня сотрудника {owner} согласовано и чем оно объясняется.",
                    user_id=user_id,
                    evidence=evidence,
                )
            )

    actions.sort(key=lambda item: (_priority_rank(item["priority"]), item["owner"].lower(), item["action_id"]))
    calendar_summary = build_report_summary(filtered_rows)
    inactive_users = sum(1 for row in roster if row["status"] == "inactive")
    portfolio_coverage_pct = _clamp_pct((workday_total_active_seconds / (expected_seconds_per_user * len(roster))) * 100.0) if roster and expected_seconds_per_user > 0 else 0.0
    calendar_first = calendar_summary.get("first_activity", "")
    calendar_last = calendar_summary.get("last_activity", "")
    return {
        "generated_at_utc": now_utc().isoformat().replace("+00:00", "Z"),
        "host": resolve_host(host),
        "report_date": report_date.isoformat(),
        "report_timezone": str(REPORT_TZ),
        "filters": {
            "owner": owner_filter,
            "department": department_filter,
        },
        "workday": {
            "start_local": workday["start_local"].isoformat(),
            "end_local": workday["end_local"].isoformat(),
            "expected_seconds_per_user": expected_seconds_per_user,
            "expected_hhmm_per_user": hhmm(expected_seconds_per_user),
            "target_coverage_pct": MANAGER_TARGET_COVERAGE_PCT,
            "low_coverage_pct": MANAGER_LOW_COVERAGE_PCT,
        },
        "summary": {
            **calendar_summary,
            "calendar_total_active_seconds": calendar_summary["total_active_seconds"],
            "calendar_total_active_hhmm": calendar_summary["total_active_hhmm"],
            "calendar_first_activity": calendar_first,
            "calendar_last_activity": calendar_last,
            "workday_total_active_seconds": workday_total_active_seconds,
            "workday_total_active_hhmm": hhmm(workday_total_active_seconds),
            "workday_first_activity": min(workday_first_values) if workday_first_values else "",
            "workday_last_activity": max(workday_last_values) if workday_last_values else "",
            "total_active_seconds": workday_total_active_seconds,
            "total_active_hhmm": hhmm(workday_total_active_seconds),
            "first_activity": min(workday_first_values) if workday_first_values else "",
            "last_activity": max(workday_last_values) if workday_last_values else "",
            "top_user": top_workday_user,
            "top_user_active_hhmm": hhmm(top_workday_seconds if top_workday_seconds > 0 else 0),
            "active_users": active_users,
            "inactive_users": inactive_users,
            "on_target_users": on_target_users,
            "below_target_users": below_target_users,
            "portfolio_coverage_pct": portfolio_coverage_pct,
            "actions_count": len(actions),
            "critical_actions_count": sum(1 for action in actions if action["priority"] == "critical"),
            "high_actions_count": sum(1 for action in actions if action["priority"] == "high"),
        },
        "actions": actions,
        "rows": roster,
        "bucket_id": get_sessions_bucket_id(host),
        "report_bounds": {
            "start_utc": to_iso_utc(report_bounds["start"]),
            "end_utc": to_iso_utc(report_bounds["end"]),
        },
    }


def _management_trend_item(payload, report_date):
    summary = payload["summary"]
    return {
        "report_date": report_date.isoformat(),
        "users_count": summary["users_count"],
        "active_users": summary["active_users"],
        "inactive_users": summary["inactive_users"],
        "workday_total_active_seconds": summary["workday_total_active_seconds"],
        "workday_total_active_hhmm": summary["workday_total_active_hhmm"],
        "portfolio_coverage_pct": summary["portfolio_coverage_pct"],
        "actions_count": summary["actions_count"],
        "critical_actions_count": summary["critical_actions_count"],
    }


def build_management_trend(host, anchor_date, owner_filter="", department_filter="", precomputed_payloads=None):
    trend = []
    precomputed_payloads = precomputed_payloads or {}
    for offset in range(MANAGER_TREND_DAYS - 1, -1, -1):
        current_date = anchor_date - timedelta(days=offset)
        payload = precomputed_payloads.get(current_date)
        if payload is None:
            payload = load_management_cache(host, current_date)
        if payload is None:
            bounds, events = fetch_events_for_date(host, current_date)
            rows = aggregate_rows_with_intervals(events, bounds["start"], bounds["end"], host)
            payload = _build_management_core(rows, host, current_date, owner_filter=owner_filter, department_filter=department_filter)
        elif owner_filter or department_filter:
            payload = apply_management_filters_to_payload(
                payload,
                owner_filter=owner_filter,
                department_filter=department_filter,
                include_sources=False,
                include_source_actions=False,
            )
        trend.append(_management_trend_item(payload, current_date))
    return trend


def build_filtered_management_trend(host, anchor_date, owner_filter="", department_filter=""):
    trend = []
    for offset in range(MANAGER_TREND_DAYS - 1, -1, -1):
        current_date = anchor_date - timedelta(days=offset)
        base_payload = load_management_cache(host, current_date)
        if base_payload is None:
            bounds, events = fetch_events_for_date(host, current_date)
            rows = aggregate_rows_with_intervals(events, bounds["start"], bounds["end"], host)
            base_payload = _build_management_core(rows, host, current_date)
        filtered_payload = apply_management_filters_to_payload(
            base_payload,
            owner_filter=owner_filter,
            department_filter=department_filter,
            include_sources=False,
            include_source_actions=False,
        )
        summary = filtered_payload["summary"]
        trend.append(
            {
                "report_date": current_date.isoformat(),
                "users_count": summary["users_count"],
                "active_users": summary["active_users"],
                "inactive_users": summary["inactive_users"],
                "workday_total_active_seconds": summary["workday_total_active_seconds"],
                "workday_total_active_hhmm": summary["workday_total_active_hhmm"],
                "portfolio_coverage_pct": summary["portfolio_coverage_pct"],
                "actions_count": summary["actions_count"],
                "critical_actions_count": summary["critical_actions_count"],
            }
        )
    return trend


def build_management_payload(rows, host, report_date, owner_filter="", department_filter=""):
    owner_filter = normalize_management_filter(owner_filter)
    department_filter = normalize_management_filter(department_filter)
    payload = _build_management_core(rows, host, report_date, owner_filter=owner_filter, department_filter=department_filter)
    source_freshness, source_actions = build_source_freshness(resolve_host(host))
    payload["sources"] = source_freshness
    payload["trend"] = build_management_trend(
        resolve_host(host),
        report_date,
        owner_filter=owner_filter,
        department_filter=department_filter,
        precomputed_payloads={report_date: payload},
    )
    payload["trend_scope"] = "portfolio"
    if source_actions:
        payload["actions"].extend(filter_management_actions(source_actions, payload["rows"], owner_filter=owner_filter, department_filter=department_filter))
        payload["actions"].sort(key=lambda item: (_priority_rank(item["priority"]), item["owner"].lower(), item["action_id"]))
        payload["summary"]["actions_count"] = len(payload["actions"])
        payload["summary"]["critical_actions_count"] = sum(1 for action in payload["actions"] if action["priority"] == "critical")
        payload["summary"]["high_actions_count"] = sum(1 for action in payload["actions"] if action["priority"] == "high")
    payload["executive"] = build_executive_summary(payload["summary"], payload["actions"], payload["sources"])
    payload["owner_rollups"] = build_owner_rollups(payload["rows"], payload["actions"])
    payload["department_rollups"] = build_department_rollups(payload["rows"], payload["actions"])
    payload["owner_roster"] = build_owner_roster(payload["rows"], payload["actions"])
    return payload


def report_for_date(host, report_date):
    bounds, events = fetch_events_for_date(host, report_date)
    return aggregate_rows(events, bounds["start"], bounds["end"], host)


def management_report_for_date(host, report_date, owner_filter="", department_filter=""):
    owner_filter = normalize_management_filter(owner_filter)
    department_filter = normalize_management_filter(department_filter)
    if owner_filter or department_filter:
        base_payload = management_report_for_date(host, report_date)
        filtered_payload = apply_management_filters_to_payload(
            base_payload,
            owner_filter=owner_filter,
            department_filter=department_filter,
            include_sources=True,
            include_source_actions=True,
        )
        filtered_payload["trend"] = []
        filtered_payload["trend_scope"] = "filtered_current_only"
        return filtered_payload
    cached = load_management_cache(host, report_date)
    if cached is not None:
        return cached
    lock = get_management_build_lock(host, report_date)
    with lock:
        cached = load_management_cache(host, report_date)
        if cached is not None:
            return cached
        bounds, events = fetch_events_for_date(host, report_date)
        rows = aggregate_rows_with_intervals(events, bounds["start"], bounds["end"], host)
        payload = build_management_payload(rows, host, report_date)
        save_management_cache(host, report_date, payload)
        return payload


def report_today(host):
    return report_for_date(host, resolve_report_date())


def report_for_date_fresh(host, report_date):
    spec = importlib.util.spec_from_file_location("aw_worktime_runtime", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.report_for_date(host, report_date)


def render_html(rows, host, report_date, selected_day=None, true_active_apps=None):
    generated = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    date_local = report_date.strftime("%Y-%m-%d")
    day_query = f"&day={selected_day}" if selected_day in {"today", "yesterday"} else ""
    date_query = f"&date={date_local}" if not day_query else ""
    summary = build_report_summary(rows)
    today_url = "/reports/worktime/today?" + urlencode({"format": "html", "host": resolve_host(host), "day": "today"})
    yesterday_url = "/reports/worktime/today?" + urlencode({"format": "html", "host": resolve_host(host), "day": "yesterday"})
    csv_url = "/reports/worktime/today?" + urlencode({"format": "csv", "host": resolve_host(host), **({"day": selected_day} if selected_day in {"today", "yesterday"} else {"date": date_local})})
    json_url = "/reports/worktime/today?" + urlencode({"host": resolve_host(host), **({"day": selected_day} if selected_day in {"today", "yesterday"} else {"date": date_local})})
    form_action = "/reports/worktime/today"
    cards = [
        ("Пользователи", str(summary["users_count"])),
        ("Активное время", summary["total_active_hhmm"]),
        ("Лидер дня", f"{summary['top_user']} · {summary['top_user_active_hhmm']}" if summary["top_user"] else "н/д"),
        ("Диапазон", f"{summary['first_activity']} -> {summary['last_activity']}" if summary["first_activity"] else "нет активности"),
    ]
    trs = []
    detail_cards = []
    true_active_apps = true_active_apps or []
    true_active_rows = []
    for app_row in true_active_apps:
        last_action = app_row.get("last_action") or "-"
        last_time = app_row.get("last_action_local") or "-"
        true_active_rows.append(
            "<tr>"
            f"<td>{html.escape(app_row.get('application') or '-')}</td>"
            f"<td class='good'>{html.escape(app_row.get('proved_work_human') or app_row.get('proved_work_hhmm') or '0 сек')}</td>"
            f"<td>{html.escape(last_time)} · {html.escape(last_action)}</td>"
            "</tr>"
        )
    if not true_active_rows:
        true_active_rows.append("<tr><td colspan='3'>Пока нет доказанной активной работы по приложениям за выбранную дату.</td></tr>")
    for row in rows:
        user_slug = safe_slug(row["user"])
        active_seconds = int(row.get("active_seconds", 0) or 0)
        utilization = 0.0
        day_total = 24 * 3600
        if day_total > 0:
            utilization = round((active_seconds / day_total) * 100.0, 2)
        trs.append(
            "<tr>"
            f"<td><a class='user-link' href='#{user_slug}'>{html.escape(row['user'])}</a></td>"
            f"<td>{html.escape(row['user_id'])}</td>"
            f"<td class='good'>{row['active_hhmm']}</td>"
            f"<td>{row['active_seconds']}</td>"
            f"<td>{html.escape(row['first_activity'])}</td>"
            f"<td>{html.escape(row['last_activity'])}</td>"
            f"<td>{row['idle_seconds']}</td>"
            f"<td>{row['sessions_count']}</td>"
            f"<td>{row['samples_count']}</td>"
            "</tr>"
        )
        detail_cards.append(
            "<article class='detail-card' id='{slug}'>"
            "<div class='detail-head'>"
            "<h3>{user}</h3>"
            "<span class='badge'>{active}</span>"
            "</div>"
            "<div class='detail-grid'>"
            "<div><span>Пользователь</span><strong>{user_id}</strong></div>"
            "<div><span>Загрузка</span><strong>{utilization}%</strong></div>"
            "<div><span>Начало активности</span><strong>{first_activity}</strong></div>"
            "<div><span>Конец активности</span><strong>{last_activity}</strong></div>"
            "<div><span>Сессии</span><strong>{sessions}</strong></div>"
            "<div><span>Активные сэмплы</span><strong>{active_samples} / {samples}</strong></div>"
            "</div>"
            "</article>"
        .format(
            slug=user_slug,
            user=html.escape(row["user"]),
            active=html.escape(row["active_hhmm"]),
            user_id=html.escape(row["user_id"]),
            utilization=utilization,
            first_activity=html.escape(row["first_activity"] or "н/д"),
            last_activity=html.escape(row["last_activity"] or "н/д"),
            sessions=row["sessions_count"],
            active_samples=row["active_samples"],
            samples=row["samples_count"],
        ))
    if not trs:
        trs.append('<tr><td colspan="9">За выбранную дату данных пока нет.</td></tr>')
        detail_cards.append("<article class='detail-card empty'><h3>За выбранную дату нет активности пользователей.</h3></article>")
    return f"""<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AW-rus Отчёт по работе в RDP</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f4f7fb;
      --card: #ffffff;
      --line: #dbe3ee;
      --text: #0f172a;
      --muted: #475569;
      --accent: #0f766e;
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font: 14px/1.45 "Segoe UI", "Noto Sans", sans-serif;
      color: var(--text);
      background:
        radial-gradient(circle at top left, rgba(29,78,216,.08), transparent 28%),
        radial-gradient(circle at top right, rgba(15,118,110,.10), transparent 24%),
        var(--bg);
    }}
    .wrap {{ max-width: 1340px; margin: 0 auto; padding: 24px; }}
    .hero {{
      background: linear-gradient(135deg, #0f172a, #1e293b 58%, #0f766e);
      color: #fff;
      border-radius: 18px;
      padding: 20px 22px;
      box-shadow: 0 22px 60px rgba(15,23,42,.22);
    }}
    .hero h1 {{ margin: 0 0 8px; font-size: 28px; }}
    .meta {{ color: rgba(255,255,255,.84); }}
    .actions {{ margin-top: 14px; display: flex; gap: 10px; flex-wrap: wrap; }}
    .actions a {{
      text-decoration: none;
      color: #fff;
      background: rgba(255,255,255,.12);
      border: 1px solid rgba(255,255,255,.18);
      padding: 8px 12px;
      border-radius: 999px;
    }}
    .toolbar {{
      margin-top: 16px;
      display: flex;
      gap: 12px;
      flex-wrap: wrap;
      align-items: center;
    }}
    .toolbar form {{
      display: flex;
      gap: 10px;
      flex-wrap: wrap;
      align-items: center;
    }}
    .toolbar input, .toolbar button {{
      border-radius: 10px;
      border: 1px solid rgba(255,255,255,.22);
      background: rgba(255,255,255,.14);
      color: #fff;
      padding: 9px 12px;
      font: inherit;
    }}
    .toolbar button {{
      cursor: pointer;
      font-weight: 600;
    }}
    .toolbar input::-webkit-calendar-picker-indicator {{ filter: invert(1); }}
    .summary-grid {{
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 14px;
      margin-top: 18px;
    }}
    .summary-card {{
      background: rgba(255,255,255,.1);
      border: 1px solid rgba(255,255,255,.14);
      border-radius: 14px;
      padding: 14px 16px;
      min-height: 96px;
    }}
    .summary-card span {{
      display: block;
      color: rgba(255,255,255,.78);
      font-size: 12px;
      margin-bottom: 8px;
      text-transform: uppercase;
      letter-spacing: .04em;
    }}
    .summary-card strong {{
      display: block;
      font-size: 22px;
      line-height: 1.25;
      word-break: break-word;
    }}
    .card {{
      margin-top: 18px;
      background: var(--card);
      border: 1px solid var(--line);
      border-radius: 16px;
      overflow: hidden;
      box-shadow: 0 16px 40px rgba(15,23,42,.08);
    }}
    table {{ width: 100%; border-collapse: collapse; }}
    th, td {{ padding: 12px 14px; border-bottom: 1px solid var(--line); text-align: left; }}
    th {{ background: #eef4fb; color: var(--muted); font-weight: 600; position: sticky; top: 0; }}
    tr:nth-child(even) td {{ background: rgba(148,163,184,.06); }}
    .good {{ color: var(--accent); font-weight: 700; }}
    .user-link {{ color: #0f4db3; text-decoration: none; font-weight: 600; }}
    .section-title {{
      margin: 0;
      padding: 18px 18px 0;
      color: var(--text);
      font-size: 18px;
    }}
    .details-wrap {{
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 16px;
      padding: 18px;
    }}
    .detail-card {{
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 16px;
      background: linear-gradient(180deg, rgba(238,244,251,.7), #fff);
      scroll-margin-top: 16px;
    }}
    .detail-card.empty {{
      grid-column: 1 / -1;
      text-align: center;
      color: var(--muted);
    }}
    .detail-head {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      margin-bottom: 14px;
    }}
    .detail-head h3 {{
      margin: 0;
      font-size: 18px;
    }}
    .badge {{
      display: inline-block;
      padding: 6px 10px;
      background: #d1fae5;
      color: #065f46;
      border-radius: 999px;
      font-weight: 700;
      font-size: 12px;
    }}
    .detail-grid {{
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 12px;
    }}
    .detail-grid span {{
      display: block;
      color: var(--muted);
      font-size: 12px;
      margin-bottom: 4px;
    }}
    .detail-grid strong {{
      display: block;
      word-break: break-word;
    }}
    @media (max-width: 900px) {{
      .wrap {{ padding: 14px; }}
      .hero h1 {{ font-size: 22px; }}
      .summary-grid {{ grid-template-columns: 1fr; }}
      .card {{ overflow-x: auto; }}
      table {{ min-width: 1080px; }}
      .details-wrap {{ grid-template-columns: 1fr; }}
      .detail-grid {{ grid-template-columns: 1fr; }}
    }}
  </style>
</head>
<body>
  <div class="wrap">
    <section class="hero">
      <h1>Отчёт по работе в RDP</h1>
      <div class="meta">Хост: {resolve_host(host)} · Дата: {date_local} · Часовой пояс: {REPORT_TZ} · Сформировано UTC: {generated}</div>
      <div class="actions">
        <a href="{today_url}">Сегодня</a>
        <a href="{yesterday_url}">Вчера</a>
        <a href="{csv_url}">Скачать CSV</a>
        <a href="{json_url}">Открыть JSON</a>
      </div>
      <div class="toolbar">
        <form method="get" action="{form_action}">
          <input type="hidden" name="format" value="html">
          <input type="hidden" name="host" value="{html.escape(resolve_host(host))}">
          <input type="date" name="date" value="{date_local}">
          <button type="submit">Открыть дату</button>
        </form>
      </div>
      <div class="summary-grid">
        {''.join(f"<div class='summary-card'><span>{html.escape(label)}</span><strong>{html.escape(value)}</strong></div>" for label, value in cards)}
      </div>
    </section>
    <section class="card">
      <h2 class="section-title">Доказанная работа по приложениям</h2>
      <table>
        <thead>
          <tr>
            <th>Приложение</th>
            <th>Доказанная работа</th>
            <th>Последнее действие</th>
          </tr>
        </thead>
        <tbody>
          {''.join(true_active_rows)}
        </tbody>
      </table>
    </section>
    <section class="card">
      <h2 class="section-title">Таблица по пользователям</h2>
      <table>
        <thead>
          <tr>
            <th>Пользователь</th>
            <th>Учётная запись</th>
            <th>Активно</th>
            <th>Активно, сек</th>
            <th>Начало активности</th>
            <th>Конец активности</th>
            <th>Простой, сек</th>
            <th>Сессии</th>
            <th>Сэмплы</th>
          </tr>
        </thead>
        <tbody>
          {''.join(trs)}
        </tbody>
      </table>
    </section>
    <section class="card">
      <h2 class="section-title">Детали по пользователям</h2>
      <div class="details-wrap">
        {''.join(detail_cards)}
      </div>
    </section>
  </div>
</body>
</html>"""


def build_management_report_url(host, report_date, selected_day=None, fmt=None, owner_filter="", department_filter=""):
    params = {"host": resolve_host(host)}
    if fmt:
        params["format"] = fmt
    if selected_day in {"today", "yesterday"}:
        params["day"] = selected_day
    else:
        params["date"] = report_date
    if normalize_management_filter(owner_filter):
        params["owner"] = normalize_management_filter(owner_filter)
    if normalize_management_filter(department_filter):
        params["department"] = normalize_management_filter(department_filter)
    return "/reports/worktime/management?" + urlencode(params)


def render_management_html(payload, selected_day=None):
    summary = payload["summary"]
    workday = payload["workday"]
    report_date = payload["report_date"]
    host = payload["host"]
    executive = payload.get("executive") or {}
    active_filters = payload.get("filters") or {}
    owner_filter = normalize_management_filter(active_filters.get("owner"))
    department_filter = normalize_management_filter(active_filters.get("department"))
    today_url = build_management_report_url(host, report_date, selected_day="today", fmt="html", owner_filter=owner_filter, department_filter=department_filter)
    yesterday_url = build_management_report_url(host, report_date, selected_day="yesterday", fmt="html", owner_filter=owner_filter, department_filter=department_filter)
    json_url = build_management_report_url(host, report_date, selected_day=selected_day, owner_filter=owner_filter, department_filter=department_filter)
    classic_url = "/reports/worktime/today?" + urlencode({"format": "html", "host": host, **({"day": selected_day} if selected_day in {"today", "yesterday"} else {"date": report_date})})
    reset_url = build_management_report_url(host, report_date, selected_day=selected_day, fmt="html")
    actions_html = []
    for action in payload["actions"]:
        actions_html.append(
            "<tr>"
            f"<td><span class='prio prio-{html.escape(action['priority'])}'>{html.escape(action['priority'])}</span></td>"
            f"<td>{html.escape(action['owner'])}</td>"
            f"<td>{html.escape(action['action_id'])}</td>"
            f"<td>{html.escape(action['deadline_hint'])}</td>"
            f"<td>{html.escape(action['reason'])}</td>"
            f"<td>{html.escape(action['recommended_action'])}</td>"
            "</tr>"
        )
    if not actions_html:
        actions_html.append("<tr><td colspan='6'>Отклонений по текущим правилам не найдено.</td></tr>")

    roster_html = []
    for row in payload["rows"]:
        roster_html.append(
            "<tr>"
            f"<td>{html.escape(row['user'])}</td>"
            f"<td>{html.escape(row.get('canonical_user_id') or row['user_id'])}</td>"
            f"<td>{html.escape(row.get('manager_owner') or row['user'])}</td>"
            f"<td>{html.escape(row.get('department') or '-')}</td>"
            f"<td>{html.escape(row['workday_active_hhmm'])}</td>"
            f"<td>{html.escape(row['calendar_active_hhmm'])}</td>"
            f"<td>{row['coverage_pct']}</td>"
            f"<td>{html.escape(row['status'])}</td>"
            f"<td>{html.escape(row.get('workday_first_activity_local') or '-')}</td>"
            f"<td>{html.escape(row.get('workday_last_activity_local') or '-')}</td>"
            f"<td>{row['sessions_count']}</td>"
            "</tr>"
        )
    if not roster_html:
        roster_html.append("<tr><td colspan='11'>За выбранную дату данных нет.</td></tr>")

    trend_html = []
    for row in payload.get("trend", []):
        trend_html.append(
            "<tr>"
            f"<td>{html.escape(row['report_date'])}</td>"
            f"<td>{row['users_count']}</td>"
            f"<td>{row['active_users']}</td>"
            f"<td>{row['inactive_users']}</td>"
            f"<td>{html.escape(row['workday_total_active_hhmm'])}</td>"
            f"<td>{row['portfolio_coverage_pct']}</td>"
            f"<td>{row['actions_count']}</td>"
            f"<td>{row['critical_actions_count']}</td>"
            "</tr>"
        )
    if not trend_html:
        trend_message = "Тренд по выбранному фильтру пока отключён, чтобы current-scope отчёт отвечал быстро." if (owner_filter or department_filter) else "Тренд пока недоступен."
        trend_html.append(f"<tr><td colspan='8'>{html.escape(trend_message)}</td></tr>")

    owner_rollup_html = []
    for item in payload.get("owner_rollups", []):
        owner_url = build_management_report_url(host, report_date, selected_day=selected_day, fmt="html", owner_filter=item["name"], department_filter=department_filter)
        owner_rollup_html.append(
            "<tr>"
            f"<td><a href='{html.escape(owner_url)}'>{html.escape(item['name'])}</a></td>"
            f"<td>{item['users_count']}</td>"
            f"<td>{item['inactive_users']}</td>"
            f"<td>{item['below_target_users']}</td>"
            f"<td>{item['critical_actions_count']}</td>"
            f"<td>{item['high_actions_count']}</td>"
            f"<td>{html.escape(item['workday_total_active_hhmm'])}</td>"
            f"<td>{item['portfolio_coverage_pct']}</td>"
            f"<td>{html.escape(', '.join(item['users']) if item['users'] else '-')}</td>"
            "</tr>"
        )
    if not owner_rollup_html:
        owner_rollup_html.append("<tr><td colspan='9'>Нет данных по ответственным.</td></tr>")

    department_rollup_html = []
    for item in payload.get("department_rollups", []):
        department_url = build_management_report_url(host, report_date, selected_day=selected_day, fmt="html", owner_filter=owner_filter, department_filter=item["name"])
        department_rollup_html.append(
            "<tr>"
            f"<td><a href='{html.escape(department_url)}'>{html.escape(item['name'])}</a></td>"
            f"<td>{item['users_count']}</td>"
            f"<td>{item['inactive_users']}</td>"
            f"<td>{item['below_target_users']}</td>"
            f"<td>{item['critical_actions_count']}</td>"
            f"<td>{item['high_actions_count']}</td>"
            f"<td>{html.escape(item['workday_total_active_hhmm'])}</td>"
            f"<td>{item['portfolio_coverage_pct']}</td>"
            f"<td>{html.escape(', '.join(item['users']) if item['users'] else '-')}</td>"
            "</tr>"
        )
    if not department_rollup_html:
        department_rollup_html.append("<tr><td colspan='9'>Нет данных по подразделениям.</td></tr>")

    owner_profile_html = []
    for item in payload.get("owner_roster", []):
        owner_profile_html.append(
            "<article class='focus-card'>"
            f"<div class='focus-priority prio prio-{'critical' if item['critical_actions_count'] > 0 else ('high' if item['high_actions_count'] > 0 else 'low')}'>{html.escape(item['display_name'])}</div>"
            f"<h3>{html.escape(item['title'] or item['name'])}</h3>"
            f"<div class='focus-owner'>Подразделение: {html.escape(item['department'] or '-')}</div>"
            f"<p>Пользователи: {item['users_count']} · inactive: {item['inactive_users']} · actions: {item['actions_count']}</p>"
            f"<p>Контакт: <strong>{html.escape(item['contact'] or '-')}</strong></p>"
            f"<p>Эскалация: <strong>{html.escape(item['escalation_to'] or '-')}</strong></p>"
            f"<p>{html.escape(item['notes'] or 'Без дополнительных заметок.')}</p>"
            "</article>"
        )
    if not owner_profile_html:
        owner_profile_html.append("<article class='focus-card'><h3>Каталог ответственных пуст</h3><p>Добавьте блок owners в worktime-manager-aliases.json, чтобы отчёт показывал роли, контакты и эскалацию.</p></article>")

    sources_html = []
    for source in payload.get("sources", []):
        sources_html.append(
            "<tr>"
            f"<td>{html.escape(source['label'])}</td>"
            f"<td><span class='prio prio-{html.escape('low' if source['status'] == 'ok' else ('critical' if source['required'] else 'medium'))}'>{html.escape(source['status_label'])}</span></td>"
            f"<td>{html.escape(source['bucket_id'])}</td>"
            f"<td>{html.escape(source.get('timestamp') or '-')}</td>"
            f"<td>{html.escape(str(source.get('age_seconds')) if source.get('age_seconds') is not None else '-')}</td>"
            f"<td>{html.escape(source.get('event_summary') or source.get('summary') or '-')}</td>"
            "</tr>"
        )
    if not sources_html:
        sources_html.append("<tr><td colspan='6'>Статусы источников недоступны.</td></tr>")

    focus_html = []
    for item in executive.get("focus_items", []):
        focus_html.append(
            "<article class='focus-card'>"
            f"<div class='focus-priority prio prio-{html.escape(item['priority'])}'>{html.escape(item['priority'])}</div>"
            f"<h3>{html.escape(item['title'])}</h3>"
            f"<div class='focus-owner'>Ответственный: {html.escape(item['owner'])}</div>"
            f"<p>{html.escape(item['reason'])}</p>"
            f"<strong>{html.escape(item['recommended_action'])}</strong>"
            "</article>"
        )
    if not focus_html:
        focus_html.append("<article class='focus-card'><h3>Критичных действий нет</h3><p>На текущий момент менеджерских отклонений по активным правилам не найдено.</p></article>")

    stale_html = []
    for item in executive.get("stale_sources", []):
        stale_html.append(
            "<li>"
            f"{html.escape(item['label'])}: {html.escape(item['status'])} · {html.escape(item['summary'])}"
            "</li>"
        )
    stale_block = (
        "<div class='note note-light'><strong>Проблемы со свежестью источников:</strong><ul>"
        + "".join(stale_html)
        + "</ul></div>"
    ) if stale_html else ""
    filter_parts = []
    if owner_filter:
        filter_parts.append(f"ответственный: {html.escape(owner_filter)}")
    if department_filter:
        filter_parts.append(f"подразделение: {html.escape(department_filter)}")
    filter_block = (
        "<div class='note note-light'><strong>Фильтр:</strong> "
        + " · ".join(filter_parts)
        + f" <a href='{html.escape(reset_url)}'>Сбросить</a></div>"
    ) if filter_parts else ""

    cards = [
        ("Пользователи", str(summary["users_count"])),
        ("Активны", str(summary["active_users"])),
        ("Без активности", str(summary["inactive_users"])),
        ("Ниже цели", str(summary["below_target_users"])),
        ("Покрытие", f"{summary['portfolio_coverage_pct']}%"),
        ("Действия", str(summary["actions_count"])),
        ("Рабочее окно", summary["workday_total_active_hhmm"]),
        ("Календарный день", summary["calendar_total_active_hhmm"]),
    ]

    return f"""<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AW-rus Управленческий отчёт по работе в RDP</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f6f8fb;
      --card: #ffffff;
      --line: #dbe3ee;
      --text: #0f172a;
      --muted: #475569;
      --accent: #0f766e;
      --critical: #b91c1c;
      --high: #c2410c;
      --medium: #1d4ed8;
      --low: #166534;
    }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; font: 14px/1.45 "Segoe UI", "Noto Sans", sans-serif; color: var(--text); background: var(--bg); }}
    .wrap {{ max-width: 1440px; margin: 0 auto; padding: 24px; }}
    .hero {{
      background: linear-gradient(135deg, #0f172a, #1e293b 58%, #0f766e);
      color: #fff;
      border-radius: 18px;
      padding: 20px 22px;
      box-shadow: 0 22px 60px rgba(15,23,42,.22);
    }}
    .hero h1 {{ margin: 0 0 8px; font-size: 28px; }}
    .meta {{ color: rgba(255,255,255,.84); }}
    .actions {{ margin-top: 14px; display: flex; gap: 10px; flex-wrap: wrap; }}
    .actions a {{
      text-decoration: none;
      color: #fff;
      background: rgba(255,255,255,.12);
      border: 1px solid rgba(255,255,255,.18);
      padding: 8px 12px;
      border-radius: 999px;
    }}
    .summary-grid {{
      display: grid;
      grid-template-columns: repeat(6, minmax(0, 1fr));
      gap: 14px;
      margin-top: 18px;
    }}
    .summary-card {{
      background: rgba(255,255,255,.1);
      border: 1px solid rgba(255,255,255,.14);
      border-radius: 14px;
      padding: 14px 16px;
    }}
    .summary-card span {{
      display: block;
      color: rgba(255,255,255,.78);
      font-size: 12px;
      margin-bottom: 8px;
      text-transform: uppercase;
      letter-spacing: .04em;
    }}
    .summary-card strong {{ display: block; font-size: 22px; }}
    .note {{
      margin-top: 16px;
      padding: 14px 16px;
      border-radius: 14px;
      background: rgba(255,255,255,.1);
      border: 1px solid rgba(255,255,255,.14);
    }}
    .note-light {{
      background: #f8fafc;
      border: 1px solid var(--line);
      color: var(--text);
    }}
    .note-light ul {{ margin: 8px 0 0 18px; padding: 0; }}
    .section {{
      margin-top: 18px;
      background: var(--card);
      border: 1px solid var(--line);
      border-radius: 16px;
      box-shadow: 0 16px 40px rgba(15,23,42,.08);
      overflow: hidden;
    }}
    .section h2 {{ margin: 0; padding: 18px; font-size: 18px; }}
    .section-body {{ padding: 18px; }}
    table {{ width: 100%; border-collapse: collapse; }}
    th, td {{ padding: 12px 14px; border-top: 1px solid var(--line); text-align: left; vertical-align: top; }}
    th {{ background: #eef4fb; color: var(--muted); font-weight: 600; }}
    .prio {{
      display: inline-block;
      padding: 4px 8px;
      border-radius: 999px;
      color: #fff;
      font-weight: 700;
      text-transform: uppercase;
      font-size: 12px;
    }}
    .prio-critical {{ background: var(--critical); }}
    .prio-high {{ background: var(--high); }}
    .prio-medium {{ background: var(--medium); }}
    .prio-low {{ background: var(--low); }}
    .focus-grid {{
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 14px;
      padding: 0 18px 18px;
    }}
    .focus-card {{
      border: 1px solid var(--line);
      border-radius: 14px;
      padding: 16px;
      background: linear-gradient(180deg, rgba(238,244,251,.8), #fff);
    }}
    .focus-card h3 {{ margin: 12px 0 8px; font-size: 16px; }}
    .focus-card p {{ margin: 0 0 10px; color: var(--muted); }}
    .focus-owner {{ color: var(--muted); font-size: 12px; }}
    .focus-priority {{ width: fit-content; }}
    @media (max-width: 1100px) {{
      .wrap {{ padding: 14px; }}
      .summary-grid {{ grid-template-columns: 1fr 1fr; }}
      .focus-grid {{ grid-template-columns: 1fr; }}
      .section {{ overflow-x: auto; }}
      table {{ min-width: 1100px; }}
    }}
    @media (max-width: 640px) {{
      .summary-grid {{ grid-template-columns: 1fr; }}
    }}
  </style>
</head>
<body>
  <div class="wrap">
    <section class="hero">
      <h1>Управленческий отчёт по работе в RDP</h1>
      <div class="meta">Хост: {html.escape(host)} · Дата: {html.escape(report_date)} · Рабочее окно: {html.escape(workday['start_local'])} -> {html.escape(workday['end_local'])} · Сформировано UTC: {html.escape(payload['generated_at_utc'])}</div>
      <div class="actions">
        <a href="{today_url}">Сегодня</a>
        <a href="{yesterday_url}">Вчера</a>
        <a href="{json_url}">Открыть JSON</a>
        <a href="{classic_url}">Классический RDP отчёт</a>
      </div>
      <div class="summary-grid">
        {''.join(f"<div class='summary-card'><span>{html.escape(label)}</span><strong>{html.escape(value)}</strong></div>" for label, value in cards)}
      </div>
      <div class="note">
        Целевое покрытие: {MANAGER_TARGET_COVERAGE_PCT}% от ожидаемого рабочего окна на пользователя.
        Критический провал: ниже {MANAGER_LOW_COVERAGE_PCT}% или полное отсутствие активности.
        Рабочее окно считается отдельно от календарной активности, чтобы ночная работа не маскировала дневной провал.
      </div>
    </section>
    <section class="section">
      <h2>Что делать сегодня</h2>
      <div class="section-body">
        <strong>{html.escape(executive.get('headline') or 'Сводка недоступна')}</strong>
        <p>{html.escape(executive.get('message') or '')}</p>
        {filter_block}
        {stale_block}
      </div>
      <div class="focus-grid">
        {''.join(focus_html)}
      </div>
    </section>
    <section class="section">
      <h2>Тренд за {MANAGER_TREND_DAYS} дней</h2>
      <table>
        <thead>
          <tr>
            <th>Дата</th>
            <th>Пользователи</th>
            <th>Активны</th>
            <th>Без активности</th>
            <th>Рабочее окно</th>
            <th>Покрытие, %</th>
            <th>Действия</th>
            <th>Critical</th>
          </tr>
        </thead>
        <tbody>
          {''.join(trend_html)}
        </tbody>
      </table>
    </section>
    <section class="section">
      <h2>По ответственным</h2>
      <table>
        <thead>
          <tr>
            <th>Ответственный</th>
            <th>Сотрудники</th>
            <th>Без активности</th>
            <th>Ниже цели</th>
            <th>Critical</th>
            <th>High</th>
            <th>Рабочее окно</th>
            <th>Покрытие, %</th>
            <th>Кого затрагивает</th>
          </tr>
        </thead>
        <tbody>
          {''.join(owner_rollup_html)}
        </tbody>
      </table>
    </section>
    <section class="section">
      <h2>Ответственные и эскалация</h2>
      <div class="focus-grid">
        {''.join(owner_profile_html)}
      </div>
    </section>
    <section class="section">
      <h2>По подразделениям</h2>
      <table>
        <thead>
          <tr>
            <th>Подразделение</th>
            <th>Сотрудники</th>
            <th>Без активности</th>
            <th>Ниже цели</th>
            <th>Critical</th>
            <th>High</th>
            <th>Рабочее окно</th>
            <th>Покрытие, %</th>
            <th>Кого затрагивает</th>
          </tr>
        </thead>
        <tbody>
          {''.join(department_rollup_html)}
        </tbody>
      </table>
    </section>
    <section class="section">
      <h2>Очередь действий руководителя</h2>
      <table>
        <thead>
          <tr>
            <th>Приоритет</th>
            <th>Сотрудник</th>
            <th>Тип</th>
            <th>Срок</th>
            <th>Почему это важно</th>
            <th>Что сделать</th>
          </tr>
        </thead>
        <tbody>
          {''.join(actions_html)}
        </tbody>
      </table>
    </section>
    <section class="section">
      <h2>Покрытие по сотрудникам</h2>
      <table>
        <thead>
          <tr>
            <th>Сотрудник</th>
            <th>Учётная запись</th>
            <th>Ответственный</th>
            <th>Подразделение</th>
            <th>Активно в окне</th>
            <th>Активно за день</th>
            <th>Покрытие, %</th>
            <th>Статус</th>
            <th>Первая активность в окне</th>
            <th>Последняя активность в окне</th>
            <th>Сессии</th>
          </tr>
        </thead>
        <tbody>
          {''.join(roster_html)}
        </tbody>
      </table>
    </section>
    <section class="section">
      <h2>Свежесть источников данных</h2>
      <table>
        <thead>
          <tr>
            <th>Источник</th>
            <th>Статус</th>
            <th>Бакет</th>
            <th>Последнее событие UTC</th>
            <th>Возраст, сек</th>
            <th>Контекст</th>
          </tr>
        </thead>
        <tbody>
          {''.join(sources_html)}
        </tbody>
      </table>
    </section>
  </div>
</body>
</html>"""


def send_bytes(handler, data, content_type, status=200):
    handler.send_response(status)
    handler.send_header("Content-Type", content_type)
    handler.send_header("Content-Length", str(len(data)))
    handler.end_headers()
    try:
        handler.wfile.write(data)
    except (BrokenPipeError, ConnectionResetError):
        return False
    return True


class H(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urlparse(self.path)
        if parsed.path in {"/health", "/api/health"}:
            data = json.dumps(worktime_health_payload(), ensure_ascii=False, indent=2).encode("utf-8")
            send_bytes(self, data, "application/json; charset=utf-8")
            return

        if parsed.path.startswith("/dlp-ioc/"):
            name = parsed.path.rsplit("/", 1)[-1]
            if name not in {"ioc_blacklist.json", "ioc_blacklist.csv", "ioc_blacklist.sql"}:
                self.send_response(404)
                self.end_headers()
                return
            path = os.path.join(IOC_DIR, name)
            if not os.path.isfile(path):
                self.send_response(404)
                self.end_headers()
                return
            with open(path, "rb") as f:
                data = f.read()
            if name.endswith(".json"):
                ctype = "application/json; charset=utf-8"
            elif name.endswith(".csv"):
                ctype = "text/csv; charset=utf-8"
            else:
                ctype = "text/plain; charset=utf-8"
            send_bytes(self, data, ctype)
            return

        if parsed.path not in {"/reports/worktime/today", "/reports/worktime/management"}:
            self.send_response(404)
            self.end_headers()
            return

        params = parse_qs(parsed.query, keep_blank_values=False)
        fmt = "json"
        if params.get("format", ["json"])[0] == "csv":
            fmt = "csv"
        elif params.get("format", ["json"])[0] == "html":
            fmt = "html"
        host = resolve_host(params.get("host", [DEFAULT_HOST])[0])
        day = params.get("day", ["today"])[0]
        date_text = params.get("date", [None])[0]
        owner_filter = normalize_management_filter(params.get("owner", [""])[0])
        department_filter = normalize_management_filter(params.get("department", [""])[0])
        report_date = resolve_report_date(day=day, date_text=date_text)
        is_management = parsed.path == "/reports/worktime/management"
        management_payload = management_report_for_date(host, report_date, owner_filter=owner_filter, department_filter=department_filter) if is_management else None
        rows = report_for_date_fresh(host, report_date) if not is_management else management_payload["rows"]
        true_active_apps = [] if is_management else build_true_active_apps(host, report_date)

        if fmt == "csv":
            if is_management:
                out = io.StringIO()
                writer = csv.DictWriter(
                    out,
                    fieldnames=["priority", "owner", "user_id", "action_id", "deadline_hint", "reason", "recommended_action"],
                    extrasaction="ignore",
                )
                writer.writeheader()
                writer.writerows(management_payload["actions"])
                data = out.getvalue().encode()
                send_bytes(self, data, "text/csv; charset=utf-8")
                return
            out = io.StringIO()
            writer = csv.DictWriter(
                out,
                fieldnames=[
                    "user",
                    "user_id",
                    "active_seconds",
                    "active_hhmm",
                    "first_activity",
                    "last_activity",
                    "idle_seconds",
                    "sessions_count",
                    "samples_count",
                    "active_samples",
                ],
            )
            writer.writeheader()
            writer.writerows(rows)
            data = out.getvalue().encode()
            send_bytes(self, data, "text/csv; charset=utf-8")
            return

        if fmt == "html":
            if is_management:
                data = render_management_html(management_payload, selected_day=day if day in {"today", "yesterday"} else None).encode("utf-8")
            else:
                data = render_html(rows, host, report_date, selected_day=day if day in {"today", "yesterday"} else None, true_active_apps=true_active_apps).encode("utf-8")
            send_bytes(self, data, "text/html; charset=utf-8")
            return

        if is_management:
            obj = management_payload
            data = json.dumps(obj, ensure_ascii=False, indent=2).encode("utf-8")
            send_bytes(self, data, "application/json; charset=utf-8")
            return

        obj = {
            "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "report_timezone": str(REPORT_TZ),
            "host": host,
            "report_date": report_date.isoformat(),
            "bucket_id": get_sessions_bucket_id(host),
            "rows": rows,
            "true_active_apps": true_active_apps,
        }
        data = json.dumps(obj, ensure_ascii=False, indent=2).encode("utf-8")
        send_bytes(self, data, "application/json; charset=utf-8")


class WorktimeHTTPServer(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 64


def main():
    WorktimeHTTPServer((LISTEN_HOST, LISTEN_PORT), H).serve_forever(poll_interval=0.5)


if __name__ == "__main__":
    main()
