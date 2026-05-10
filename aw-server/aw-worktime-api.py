#!/usr/bin/env python3
from http.server import BaseHTTPRequestHandler, HTTPServer
import csv
import io
import json
import os
import urllib.request
from datetime import datetime, timezone, timedelta
from zoneinfo import ZoneInfo

AW = "http://127.0.0.1:5600/api/0"
REPORT_TZ = ZoneInfo(os.environ.get("AW_WORKTIME_TZ", "Europe/Moscow"))
IOC_DIR = os.environ.get("AW_DLP_IOC_DIR", "/opt/activitywatch/dlp-ioc/output")


def get(u):
    with urllib.request.urlopen(u, timeout=30) as r:
        return json.loads(r.read().decode())


def pts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)


def _is_machine_user(user: str) -> bool:
    u = (user or "").strip().lower()
    return u.endswith("$") or u in {"system", "localservice", "networkservice"}


def _is_active_sample(data: dict) -> bool:
    state = str(data.get("state") or "").strip().lower()
    if isinstance(data.get("active"), bool):
        if data.get("active"):
            return True
    if ("актив" in state) or (state == "active"):
        return True
    # query user can intermittently return "Unknown" on RDP hosts; if session id is valid
    # and user is not a machine/service account, treat it as activity sample.
    if state == "unknown":
        try:
            sid = int(data.get("sessionId"))
        except Exception:
            sid = -1
        user = str(data.get("username") or "").strip()
        if sid > 0 and user and (not _is_machine_user(user)):
            return True
    return False


def report_today():
    now_local = datetime.now(REPORT_TZ)
    start_local = datetime(now_local.year, now_local.month, now_local.day, tzinfo=REPORT_TZ)
    end_local = start_local + timedelta(days=1) - timedelta(seconds=1)
    start = start_local.astimezone(timezone.utc)
    end = end_local.astimezone(timezone.utc)
    b = get(AW + "/buckets")
    sb = next((k for k in b if k.startswith("aw-worktime-sessions_")), None)
    if not sb:
        return []
    ev = get(f"{AW}/buckets/{sb}/events?limit=50000")
    by = {}
    for e in ev:
        ts = pts(e.get("timestamp"))
        if ts < start or ts > end:
            continue
        d = e.get("data") or {}
        user = (d.get("username") or "").strip()
        if not user:
            continue
        active = _is_active_sample(d)
        row = by.setdefault(user, {"active": set(), "first": None, "last": None, "rows": 0})
        row["rows"] += 1
        if active:
            second = ts.replace(microsecond=0)
            row["active"].add(second)
            row["first"] = second if row["first"] is None or second < row["first"] else row["first"]
            row["last"] = second if row["last"] is None or second > row["last"] else row["last"]
    rows = []
    full = int((end_local - start_local).total_seconds())
    for user in sorted(by):
        row = by[user]
        active_seconds = len(row["active"])
        rows.append({
            "user": user,
            "active_seconds": active_seconds,
            "active_hhmm": "%02d:%02d" % (active_seconds // 3600, (active_seconds % 3600) // 60),
            "first_activity": row["first"].isoformat().replace("+00:00", "Z") if row["first"] else "",
            "last_activity": row["last"].isoformat().replace("+00:00", "Z") if row["last"] else "",
            "idle_seconds": max(0, full - active_seconds),
            "sessions_count": row["rows"],
        })
    return rows


def render_html(rows):
    generated = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    date_local = datetime.now(REPORT_TZ).strftime("%Y-%m-%d")
    trs = []
    for row in rows:
        trs.append(
            "<tr>"
            f"<td>{row['user']}</td>"
            f"<td>{row['active_hhmm']}</td>"
            f"<td>{row['active_seconds']}</td>"
            f"<td>{row['first_activity']}</td>"
            f"<td>{row['last_activity']}</td>"
            f"<td>{row['idle_seconds']}</td>"
            f"<td>{row['sessions_count']}</td>"
            "</tr>"
        )
    if not trs:
        trs.append('<tr><td colspan="7">No data for today yet.</td></tr>')
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
      --accent-2: #1d4ed8;
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
    .wrap {{ max-width: 1180px; margin: 0 auto; padding: 24px; }}
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
    .num {{ font-variant-numeric: tabular-nums; }}
    .good {{ color: var(--accent); font-weight: 700; }}
    .muted {{ color: var(--muted); }}
    @media (max-width: 900px) {{
      .wrap {{ padding: 14px; }}
      .hero h1 {{ font-size: 22px; }}
      .card {{ overflow-x: auto; }}
      table {{ min-width: 820px; }}
    }}
  </style>
</head>
<body>
  <div class="wrap">
    <section class="hero">
      <h1>RDP Worktime Report</h1>
      <div class="meta">Date: {date_local} · Timezone: {REPORT_TZ} · Generated UTC: {generated}</div>
      <div class="actions">
        <a href="/reports/worktime/today?format=csv">Download CSV</a>
        <a href="/reports/worktime/today">View JSON</a>
      </div>
    </section>
    <section class="card">
      <table>
        <thead>
          <tr>
            <th>User</th>
            <th>Active</th>
            <th>Active sec</th>
            <th>First activity</th>
            <th>Last activity</th>
            <th>Idle sec</th>
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
        if self.path.startswith("/dlp-ioc/"):
            name = self.path.split("?", 1)[0].rsplit("/", 1)[-1]
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

        if not self.path.startswith("/reports/worktime/today"):
            self.send_response(404)
            self.end_headers()
            return
        fmt = "json"
        if "format=csv" in self.path:
            fmt = "csv"
        elif "format=html" in self.path:
            fmt = "html"
        rows = report_today()
        if fmt == "csv":
            out = io.StringIO()
            writer = csv.DictWriter(
                out,
                fieldnames=[
                    "user",
                    "active_seconds",
                    "active_hhmm",
                    "first_activity",
                    "last_activity",
                    "idle_seconds",
                    "sessions_count",
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
            data = render_html(rows).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        obj = {
            "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "report_timezone": str(REPORT_TZ),
            "rows": rows,
        }
        data = json.dumps(obj, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


HTTPServer(("0.0.0.0", 5610), H).serve_forever()
