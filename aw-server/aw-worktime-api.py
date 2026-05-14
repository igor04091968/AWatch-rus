#!/usr/bin/env python3
import csv
import io
import importlib.util
import json
import os
import sys
import urllib.request
from datetime import datetime, timezone, timedelta
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse
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


def aggregate_rows(events, start, end, host):
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
                interval_end = min(sample["_ts"] + timedelta(seconds=sample_seconds), end + timedelta(seconds=1))
                if interval_end > interval_start:
                    row["intervals"].append((interval_start, interval_end))

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


def report_for_date(host, report_date):
    start_local = datetime(report_date.year, report_date.month, report_date.day, tzinfo=REPORT_TZ)
    end_local = start_local + timedelta(days=1) - timedelta(seconds=1)
    start = start_local.astimezone(timezone.utc)
    end = end_local.astimezone(timezone.utc)
    bucket_id = get_sessions_bucket_id(host)
    try:
        get(f"{AW}/buckets/{bucket_id}")
    except Exception:
        log_warning(f"bucket lookup failed for host={host} bucket={bucket_id} aw_base={AW}")
        return []
    try:
        events = get(f"{AW}/buckets/{bucket_id}/events?limit=50000")
    except Exception:
        log_warning(f"events fetch failed for host={host} bucket={bucket_id} aw_base={AW}")
        return []
    return aggregate_rows(events, start, end, host)


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
    trs = []
    for row in rows:
        trs.append(
            "<tr>"
            f"<td>{row['user']}</td>"
            f"<td>{row['user_id']}</td>"
            f"<td class='good'>{row['active_hhmm']}</td>"
            f"<td>{row['active_seconds']}</td>"
            f"<td>{row['first_activity']}</td>"
            f"<td>{row['last_activity']}</td>"
            f"<td>{row['idle_seconds']}</td>"
            f"<td>{row['sessions_count']}</td>"
            f"<td>{row['samples_count']}</td>"
            "</tr>"
        )
    if not trs:
        trs.append('<tr><td colspan="9">No data for today yet.</td></tr>')
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AW-rus Worktime</title>
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
    @media (max-width: 900px) {{
      .wrap {{ padding: 14px; }}
      .hero h1 {{ font-size: 22px; }}
      .card {{ overflow-x: auto; }}
      table {{ min-width: 1080px; }}
    }}
  </style>
</head>
<body>
  <div class="wrap">
    <section class="hero">
      <h1>RDP Worktime Report</h1>
      <div class="meta">Host: {resolve_host(host)} · Date: {date_local} · Timezone: {REPORT_TZ} · Generated UTC: {generated}</div>
      <div class="actions">
        <a href="/reports/worktime/today?format=html&host={resolve_host(host)}&day=today">Today</a>
        <a href="/reports/worktime/today?format=html&host={resolve_host(host)}&day=yesterday">Yesterday</a>
        <a href="/reports/worktime/today?format=csv&host={resolve_host(host)}{day_query}{date_query}">Download CSV</a>
        <a href="/reports/worktime/today?host={resolve_host(host)}{day_query}{date_query}">View JSON</a>
      </div>
    </section>
    <section class="card">
      <table>
        <thead>
          <tr>
            <th>User</th>
            <th>User ID</th>
            <th>Active</th>
            <th>Active sec</th>
            <th>First activity</th>
            <th>Last activity</th>
            <th>Idle sec</th>
            <th>Sessions</th>
            <th>Samples</th>
          </tr>
        </thead>
        <tbody>
          {''.join(trs)}
        </tbody>
      </table>
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
