#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from urllib.parse import quote
from urllib.request import Request, urlopen


def _env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


AW_API_BASE = _env("AW_SERVER_URL", "http://127.0.0.1:5600/api/0").rstrip("/")
OUTPUT_DIR = Path(_env("AW_DLP_COMPLIANCE_REPORT_DIR", "/opt/activitywatch/dlp-compliance/reports"))
TEMPLATE_PATH = Path(_env("AW_DLP_COMPLIANCE_TEMPLATE", "/opt/activitywatch/dlp-compliance/templates/152-fz-report.html"))


@dataclass
class ReportStats:
    total_incidents: int
    high: int
    medium: int
    low: int
    by_host: dict[str, int]
    channels: dict[str, int]


def _http_json(url: str) -> object:
    req = Request(url, headers={"Accept": "application/json"})
    with urlopen(req, timeout=30) as response:
        return json.loads(response.read().decode("utf-8"))


def _parse_ts(value: str | None) -> datetime | None:
    if not value:
        return None
    text = value.replace("Z", "+00:00")
    try:
        return datetime.fromisoformat(text).astimezone(UTC)
    except ValueError:
        return None


def _load_incidents(start: datetime, end: datetime) -> list[dict]:
    buckets = _http_json(f"{AW_API_BASE}/buckets")
    if not isinstance(buckets, dict):
        return []
    bucket_ids = sorted([bid for bid in buckets.keys() if str(bid).startswith("aw-dlp-incidents_")])

    incidents: list[dict] = []
    for bucket_id in bucket_ids:
        encoded = quote(str(bucket_id), safe="")
        events = _http_json(f"{AW_API_BASE}/buckets/{encoded}/events?limit=2000")
        if not isinstance(events, list):
            continue
        for event in events:
            if not isinstance(event, dict):
                continue
            ts = _parse_ts(event.get("timestamp"))
            if ts is None or ts < start or ts > end:
                continue
            incidents.append(event)
    return incidents


def _build_stats(incidents: list[dict]) -> ReportStats:
    by_host: dict[str, int] = {}
    channels: dict[str, int] = {}
    high = medium = low = 0
    for event in incidents:
        data = event.get("data") or {}
        if not isinstance(data, dict):
            data = {}
        host = str(data.get("hostname") or "unknown")
        by_host[host] = by_host.get(host, 0) + 1

        severity = str(data.get("severity") or "low").lower()
        if severity == "high":
            high += 1
        elif severity == "medium":
            medium += 1
        else:
            low += 1

        channel = str(data.get("signalType") or data.get("source") or "unknown")
        channels[channel] = channels.get(channel, 0) + 1

    return ReportStats(
        total_incidents=len(incidents),
        high=high,
        medium=medium,
        low=low,
        by_host=dict(sorted(by_host.items(), key=lambda item: item[1], reverse=True)),
        channels=dict(sorted(channels.items(), key=lambda item: item[1], reverse=True)),
    )


def _render_table(title: str, rows: list[tuple[str, int]]) -> str:
    if not rows:
        return f"<h3>{title}</h3><p>Нет данных</p>"
    body = "".join([f"<tr><td>{name}</td><td>{count}</td></tr>" for name, count in rows])
    return f"<h3>{title}</h3><table><thead><tr><th>Параметр</th><th>Значение</th></tr></thead><tbody>{body}</tbody></table>"


def _render_html(period_label: str, stats: ReportStats, generated_at: str) -> str:
    template = TEMPLATE_PATH.read_text(encoding="utf-8")
    return (
        template.replace("{{PERIOD}}", period_label)
        .replace("{{GENERATED_AT}}", generated_at)
        .replace("{{TOTAL}}", str(stats.total_incidents))
        .replace("{{HIGH}}", str(stats.high))
        .replace("{{MEDIUM}}", str(stats.medium))
        .replace("{{LOW}}", str(stats.low))
        .replace("{{HOST_TABLE}}", _render_table("Инциденты по хостам", list(stats.by_host.items())))
        .replace("{{CHANNEL_TABLE}}", _render_table("Инциденты по каналам", list(stats.channels.items())))
    )


def _period_bounds(month: str | None) -> tuple[datetime, datetime, str]:
    if month:
        start = datetime.fromisoformat(f"{month}-01T00:00:00+00:00").astimezone(UTC)
    else:
        now = datetime.now(UTC)
        start = now.replace(day=1, hour=0, minute=0, second=0, microsecond=0)
    if start.month == 12:
        end = start.replace(year=start.year + 1, month=1)
    else:
        end = start.replace(month=start.month + 1)
    end = end.replace(second=0, microsecond=0)
    return start, end, start.strftime("%Y-%m")


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate 152-FZ compliance report from AW DLP incidents")
    parser.add_argument("--month", help="Month in YYYY-MM format (default: current month)")
    parser.add_argument("--stdout-json", action="store_true", help="Print report metadata as JSON")
    args = parser.parse_args()

    start, end, period_label = _period_bounds(args.month)
    incidents = _load_incidents(start, end)
    stats = _build_stats(incidents)

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    generated_at = datetime.now(UTC).isoformat().replace("+00:00", "Z")
    html_out = OUTPUT_DIR / f"152-fz-{period_label}.html"
    html_out.write_text(_render_html(period_label, stats, generated_at), encoding="utf-8")

    metadata = {
        "period": period_label,
        "generated_at": generated_at,
        "aw_api_base": AW_API_BASE,
        "report_path": str(html_out),
        "stats": {
            "total_incidents": stats.total_incidents,
            "high": stats.high,
            "medium": stats.medium,
            "low": stats.low,
        },
    }
    (OUTPUT_DIR / f"152-fz-{period_label}.json").write_text(
        json.dumps(metadata, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    if args.stdout_json:
        print(json.dumps(metadata, ensure_ascii=False))


if __name__ == "__main__":
    main()
