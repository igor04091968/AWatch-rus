#!/usr/bin/env python3
import importlib.util
import os
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("aw-worktime-api.py")
SPEC = importlib.util.spec_from_file_location("aw_worktime_api", MODULE_PATH)
WORKTIME = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WORKTIME)

INFLUX_URL = os.environ.get("AW_WORKTIME_INFLUX_URL", "").strip().rstrip("/")
INFLUX_ORG = os.environ.get("AW_WORKTIME_INFLUX_ORG", "proxmox").strip() or "proxmox"
INFLUX_BUCKET = os.environ.get("AW_WORKTIME_INFLUX_BUCKET", "aw_metrics").strip() or "aw_metrics"
INFLUX_TOKEN = os.environ.get("AW_WORKTIME_INFLUX_TOKEN", "").strip()
INFLUX_ENABLED = os.environ.get("AW_WORKTIME_INFLUX_ENABLED", "").strip().lower() in {"1", "true", "yes", "on"}
HOSTS = [item.strip() for item in os.environ.get("AW_WORKTIME_INFLUX_HOSTS", WORKTIME.DEFAULT_HOST).split(",") if item.strip()]
DAYS = [item.strip() for item in os.environ.get("AW_WORKTIME_INFLUX_DAYS", "today,yesterday").split(",") if item.strip()]


def _escape_tag(value):
    return (
        str(value or "")
        .replace("\\", "\\\\")
        .replace(" ", "\\ ")
        .replace(",", "\\,")
        .replace("=", "\\=")
    )


def _line(measurement, tags, fields, timestamp_ns):
    tag_part = ",".join(f"{key}={_escape_tag(value)}" for key, value in sorted(tags.items()) if value is not None and value != "")
    field_parts = []
    for key, value in fields.items():
        if isinstance(value, bool):
            field_parts.append(f"{key}={'true' if value else 'false'}")
        elif isinstance(value, int):
            field_parts.append(f"{key}={value}i")
        elif isinstance(value, float):
            field_parts.append(f"{key}={value}")
        else:
            text = str(value or "").replace("\\", "\\\\").replace('"', '\\"')
            field_parts.append(f'{key}="{text}"')
    if not field_parts:
        return ""
    if tag_part:
        return f"{measurement},{tag_part} {','.join(field_parts)} {timestamp_ns}"
    return f"{measurement} {','.join(field_parts)} {timestamp_ns}"


def _timestamp_ns(dt):
    return int(dt.astimezone(timezone.utc).timestamp() * 1_000_000_000)


def build_lines_for_day(host, report_date):
    bounds, events = WORKTIME.fetch_events_for_date(host, report_date)
    rows = WORKTIME.aggregate_rows(events, bounds["start"], bounds["end"], host)
    hourly_rows = WORKTIME.aggregate_hourly_rows(events, bounds["start"], bounds["end"], host)
    summary = WORKTIME.build_report_summary(rows)

    lines = []
    daily_ts = _timestamp_ns(bounds["start"])

    for row in rows:
        lines.append(
            _line(
                "aw_rdp_worktime_daily",
                {
                    "host": host,
                    "user": row["user"],
                    "user_id": row["user_id"],
                    "report_date": report_date.isoformat(),
                },
                {
                    "active_seconds": int(row["active_seconds"]),
                    "idle_seconds": int(row["idle_seconds"]),
                    "sessions_count": int(row["sessions_count"]),
                    "samples_count": int(row["samples_count"]),
                    "active_samples": int(row["active_samples"]),
                },
                daily_ts,
            )
        )

    for row in hourly_rows:
        lines.append(
            _line(
                "aw_rdp_worktime_hourly",
                {
                    "host": host,
                    "user": row["user"],
                    "user_id": row["user_id"],
                    "report_date": row["report_date"],
                    "hour_local": row["hour_local"],
                },
                {
                    "active_seconds": int(row["active_seconds"]),
                },
                _timestamp_ns(WORKTIME.pts(row["bucket_start_utc"])),
            )
        )

    lines.append(
        _line(
            "aw_rdp_worktime_summary_daily",
            {
                "host": host,
                "report_date": report_date.isoformat(),
            },
            {
                "users_count": int(summary["users_count"]),
                "total_active_seconds": int(summary["total_active_seconds"]),
                "top_user": summary["top_user"],
            },
            daily_ts,
        )
    )
    return [line for line in lines if line]


def write_lines(lines):
    if not lines:
        return 0
    if not INFLUX_URL or not INFLUX_TOKEN:
        raise RuntimeError("InfluxDB destination is not configured")
    payload = ("\n".join(lines) + "\n").encode("utf-8")
    req = urllib.request.Request(
        f"{INFLUX_URL}/api/v2/write?org={INFLUX_ORG}&bucket={INFLUX_BUCKET}&precision=ns",
        data=payload,
        method="POST",
        headers={"Authorization": f"Token {INFLUX_TOKEN}", "Content-Type": "text/plain; charset=utf-8"},
    )
    with urllib.request.urlopen(req, timeout=30) as response:
        if response.status not in {204, 200}:
            raise RuntimeError(f"InfluxDB write failed with status={response.status}")
    return len(lines)


def main():
    if not INFLUX_ENABLED:
        print("[aw-worktime-influx-exporter] disabled by AW_WORKTIME_INFLUX_ENABLED", file=sys.stderr)
        return 0

    lines = []
    for host in HOSTS:
        for day in DAYS:
            report_date = WORKTIME.resolve_report_date(day=day)
            lines.extend(build_lines_for_day(host, report_date))
    written = write_lines(lines)
    print(f"[aw-worktime-influx-exporter] wrote {written} points to {INFLUX_BUCKET}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
