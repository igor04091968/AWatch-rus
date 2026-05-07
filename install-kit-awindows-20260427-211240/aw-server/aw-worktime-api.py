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


def get(u):
    with urllib.request.urlopen(u, timeout=30) as r:
        return json.loads(r.read().decode())


def pts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)


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
        state = (d.get("state") or "").lower()
        active = ("актив" in state) or (state == "active")
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


class H(BaseHTTPRequestHandler):
    def do_GET(self):
        if not self.path.startswith("/reports/worktime/today"):
            self.send_response(404)
            self.end_headers()
            return
        fmt = "json"
        if "format=csv" in self.path:
            fmt = "csv"
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
