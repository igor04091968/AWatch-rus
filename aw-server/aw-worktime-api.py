#!/usr/bin/env python3
import csv
import html
import io
import importlib.util
import json
import os
import sys
import urllib.request
from datetime import datetime, timezone, timedelta
from http.server import BaseHTTPRequestHandler, HTTPServer
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
MODULE_PATH = Path(__file__).resolve()


def get(u):
    with urllib.request.urlopen(u, timeout=30) as r:
        return json.loads(r.read().decode())


def log_warning(message):
    print(f"[aw-worktime-api] {message}", file=sys.stderr, flush=True)


def pts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)


def to_iso_utc(dt):
    return dt.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def hhmm(total_seconds):
    total_seconds = max(0, int(total_seconds))
    return "%02d:%02d" % (total_seconds // 3600, (total_seconds % 3600) // 60)


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
    try:
        get(f"{AW}/buckets/{bucket_id}")
    except Exception:
        log_warning(f"bucket lookup failed for host={host} bucket={bucket_id} aw_base={AW}")
        return bounds, []
    try:
        events = get(f"{AW}/buckets/{bucket_id}/events?limit=50000")
    except Exception:
        log_warning(f"events fetch failed for host={host} bucket={bucket_id} aw_base={AW}")
        return bounds, []
    return bounds, events


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


def report_for_date(host, report_date):
    bounds, events = fetch_events_for_date(host, report_date)
    return aggregate_rows(events, bounds["start"], bounds["end"], host)


def report_today(host):
    return report_for_date(host, resolve_report_date())


def report_for_date_fresh(host, report_date):
    spec = importlib.util.spec_from_file_location("aw_worktime_runtime", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.report_for_date(host, report_date)


def render_html(rows, host, report_date, selected_day=None):
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


class H(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urlparse(self.path)
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
            self.send_response(200)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return

        if parsed.path != "/reports/worktime/today":
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
        report_date = resolve_report_date(day=day, date_text=date_text)
        rows = report_for_date_fresh(host, report_date)

        if fmt == "csv":
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
            self.send_response(200)
            self.send_header("Content-Type", "text/csv; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return

        if fmt == "html":
            data = render_html(rows, host, report_date, selected_day=day if day in {"today", "yesterday"} else None).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return

        obj = {
            "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "report_timezone": str(REPORT_TZ),
            "host": host,
            "report_date": report_date.isoformat(),
            "bucket_id": get_sessions_bucket_id(host),
            "rows": rows,
        }
        data = json.dumps(obj, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def main():
    HTTPServer((LISTEN_HOST, LISTEN_PORT), H).serve_forever()


if __name__ == "__main__":
    main()
