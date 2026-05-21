#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import urllib.error
import urllib.parse
import urllib.request
from collections import Counter
from datetime import datetime, timezone
from typing import Any


SEVERITY_ORDER = {"low": 1, "medium": 2, "high": 3, "critical": 4}
LEVEL_NORMALIZATION = {
    "informational": "info",
    "info": "info",
    "low": "low",
    "med": "med",
    "medium": "med",
    "high": "high",
    "crit": "crit",
    "critical": "crit",
}
LEVEL_WEIGHTS = {
    "info": 1,
    "low": 4,
    "med": 12,
    "high": 40,
    "crit": 100,
}


def env_bool(name: str, default: bool) -> bool:
    value = os.environ.get(name)
    if value is None:
        return default
    return value.strip().lower() in {"1", "true", "yes", "on"}


def normalize_level(level: str | None) -> str:
    if not level:
        return "info"
    return LEVEL_NORMALIZATION.get(level.strip().lower(), level.strip().lower())


def severity_meets(actual: str, threshold: str) -> bool:
    return SEVERITY_ORDER.get(actual, 0) >= SEVERITY_ORDER.get(threshold, 0)


def build_hayabusa_payload(intake: dict[str, Any], mode: str, link_source: str) -> dict[str, Any]:
    report_dir = pathlib.Path(intake["report_dir"])
    return {
        "tool": "hayabusa",
        "host": intake["host"],
        "mode": mode,
        "status": intake["status"],
        "intake_id": intake["intake_id"],
        "package_path": intake["package_path"],
        "sha256": intake["sha256"],
        "report_dir": intake["report_dir"],
        "summary_html": str(report_dir / "summary.html"),
        "timeline_path": str(report_dir / "timeline.jsonl"),
        "manifest_path": str(report_dir / "manifest.json"),
        "link_source": link_source,
    }


def post_json(url: str, payload: dict[str, Any]) -> dict[str, Any]:
    data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(url, data=data, method="POST", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        body = resp.read().decode("utf-8")
        return json.loads(body) if body else {}


def patch_json(url: str, payload: dict[str, Any]) -> dict[str, Any]:
    data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(url, data=data, method="PATCH", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        body = resp.read().decode("utf-8")
        return json.loads(body) if body else {}


def get_json(url: str) -> dict[str, Any]:
    with urllib.request.urlopen(url) as resp:
        return json.loads(resp.read().decode("utf-8"))


def parse_timestamp(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def read_csv_rows(path: pathlib.Path) -> int:
    if not path.is_file():
        return 0
    with path.open("r", encoding="utf-8-sig", newline="") as fh:
        reader = csv.reader(fh)
        rows = list(reader)
    if not rows:
        return 0
    return max(0, len(rows) - 1)


def analyze_report(report_dir: pathlib.Path) -> dict[str, Any]:
    timeline_path = report_dir / "timeline.jsonl"
    level_counts: Counter[str] = Counter()
    title_counts: Counter[str] = Counter()
    first_ts: datetime | None = None
    last_ts: datetime | None = None
    total_events = 0

    if timeline_path.is_file():
        with timeline_path.open("r", encoding="utf-8") as fh:
            for raw_line in fh:
                line = raw_line.strip()
                if not line:
                    continue
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue
                total_events += 1
                level = normalize_level(str(event.get("Level", "")))
                level_counts[level] += 1
                title = str(event.get("RuleTitle") or "").strip() or "Unknown rule"
                title_counts[title] += 1
                ts = parse_timestamp(event.get("Timestamp"))
                if ts is not None:
                    first_ts = ts if first_ts is None or ts < first_ts else first_ts
                    last_ts = ts if last_ts is None or ts > last_ts else last_ts

    failed_logons = read_csv_rows(report_dir / "logon-summary-failed.csv")
    successful_logons = read_csv_rows(report_dir / "logon-summary-successful.csv")

    suspicious_pwsh = sum(
        count
        for title, count in title_counts.items()
        if "pwsh" in title.lower() or "powershell" in title.lower() or "obfuscation" in title.lower()
    )
    credential_events = sum(count for title, count in title_counts.items() if "credential" in title.lower())
    timestomp_events = sum(count for title, count in title_counts.items() if "timestomp" in title.lower())
    logon_failure_events = sum(count for title, count in title_counts.items() if "logon failure" in title.lower())

    score = (
        sum(LEVEL_WEIGHTS.get(level, 0) * count for level, count in level_counts.items())
        + min(failed_logons, 200) * 2
        + suspicious_pwsh * 6
        + credential_events * 8
        + timestomp_events * 12
        + logon_failure_events * 2
    )

    crit_count = level_counts.get("crit", 0)
    high_count = level_counts.get("high", 0)
    med_count = level_counts.get("med", 0)

    if crit_count >= 1 or score >= 240 or (high_count >= 4 and suspicious_pwsh >= 4):
        severity = "critical"
    elif high_count >= 1 or score >= 120 or suspicious_pwsh >= 8 or credential_events >= 5:
        severity = "high"
    elif med_count >= 1 or score >= 40 or failed_logons >= 10:
        severity = "medium"
    else:
        severity = "low"

    return {
        "severity": severity,
        "score": score,
        "events_total": total_events,
        "level_counts": dict(level_counts),
        "top_rules": [{"title": title, "count": count} for title, count in title_counts.most_common(5)],
        "first_timestamp": first_ts.astimezone(timezone.utc).isoformat().replace("+00:00", "Z") if first_ts else None,
        "last_timestamp": last_ts.astimezone(timezone.utc).isoformat().replace("+00:00", "Z") if last_ts else None,
        "failed_logon_rows": failed_logons,
        "successful_logon_rows": successful_logons,
        "suspicious_pwsh": suspicious_pwsh,
        "credential_events": credential_events,
        "timestomp_events": timestomp_events,
        "logon_failure_events": logon_failure_events,
    }


def build_case_title(host: str, summary: dict[str, Any]) -> str:
    top_rule = summary.get("top_rules") or []
    suffix = top_rule[0]["title"] if top_rule else "No dominant rule"
    return f"Hayabusa {summary['severity'].upper()} · {host} · {suffix}"


def build_case_payload(intake: dict[str, Any], summary: dict[str, Any]) -> dict[str, Any]:
    return {
        "incident_id": f"hayabusa:{intake['host']}:{intake['intake_id']}",
        "host": intake["host"],
        "title": build_case_title(intake["host"], summary),
        "severity": summary["severity"],
        "evidence": {
            "hayabusa": {
                "intake_id": intake["intake_id"],
                "package_path": intake["package_path"],
                "sha256": intake["sha256"],
                "report_dir": intake["report_dir"],
                "summary": summary,
            }
        },
    }


def build_comment(summary: dict[str, Any], intake: dict[str, Any]) -> str:
    top = ", ".join(f"{item['title']} ({item['count']})" for item in summary.get("top_rules", [])[:3]) or "n/a"
    return (
        f"Hayabusa auto-summary\n"
        f"Severity: {summary['severity']} (score={summary['score']})\n"
        f"Host: {intake['host']}\n"
        f"Intake: {intake['intake_id']}\n"
        f"Events: {summary['events_total']}, failed_logons={summary['failed_logon_rows']}, "
        f"suspicious_pwsh={summary['suspicious_pwsh']}, credential_events={summary['credential_events']}\n"
        f"Top rules: {top}\n"
        f"Report: {intake['report_dir']}"
    )


def send_telegram(bot_token: str, chat_ids: list[str], text: str) -> list[dict[str, Any]]:
    results = []
    for chat_id in chat_ids:
        payload = urllib.parse.urlencode({"chat_id": chat_id, "text": text}).encode("utf-8")
        url = f"https://api.telegram.org/bot{bot_token}/sendMessage"
        req = urllib.request.Request(url, data=payload, method="POST")
        try:
            with urllib.request.urlopen(req) as resp:
                body = json.loads(resp.read().decode("utf-8"))
            results.append({"chat_id": chat_id, "ok": True, "response": body})
        except Exception as exc:  # noqa: BLE001
            results.append({"chat_id": chat_id, "ok": False, "error": str(exc)})
    return results


def build_telegram_text(case_id: int | None, intake: dict[str, Any], summary: dict[str, Any]) -> str:
    severity_label = {
        "critical": "критичное событие",
        "high": "опасное событие",
        "medium": "подозрительное событие",
        "low": "слабый сигнал",
    }.get(summary["severity"], summary["severity"])
    top_rules = summary.get("top_rules", [])
    top_rule = top_rules[0]["title"] if top_rules else "нет явного доминирующего правила"
    lines = [
        f"Hayabusa: {severity_label}",
        "",
        f"Хост: {intake['host']}",
    ]
    if case_id is not None:
        lines.append(f"Кейс: {case_id}")
    lines.extend(
        [
            f"Уровень: {summary['severity']}",
            "",
            "Что найдено:",
            f"- {top_rule}: {top_rules[0]['count'] if top_rules else 0}",
            f"- подозрительный PowerShell: {summary['suspicious_pwsh']}",
            f"- ошибок входа: {summary['logon_failure_events']}",
            f"- событий по учётным данным: {summary['credential_events']}",
        ]
    )
    if summary.get("timestomp_events", 0) > 0:
        lines.append(f"- timestomp-подобных событий: {summary['timestomp_events']}")
    lines.extend(
        [
            "",
            "Главный риск:",
            "возможная активность вокруг учётных данных и PowerShell",
            "",
            "Отчёт:",
            f"{intake['report_dir']}",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    p = argparse.ArgumentParser(description="Auto-create/update AW-rus case, compute Hayabusa severity, and send Telegram alerts")
    p.add_argument("--case-id", type=int)
    p.add_argument("--intake-json", default="/opt/hayabusa/state/latest-intake.json")
    p.add_argument("--case-api-base", default=os.environ.get("AW_HAYABUSA_CASE_API_BASE", "http://127.0.0.1:5602"))
    p.add_argument("--mode", default="incident")
    p.add_argument("--link-source", default="aw-rus-drop-autoprocess")
    p.add_argument("--auto-create", action="store_true", default=env_bool("AW_HAYABUSA_AUTO_CASE_ENABLED", True))
    p.add_argument("--auto-create-min-severity", default=os.environ.get("AW_HAYABUSA_AUTO_CASE_MIN_SEVERITY", "medium"))
    p.add_argument("--telegram-enabled", action="store_true", default=env_bool("AW_HAYABUSA_TELEGRAM_ENABLED", False))
    p.add_argument("--telegram-min-severity", default=os.environ.get("AW_HAYABUSA_TELEGRAM_MIN_SEVERITY", "high"))
    p.add_argument("--telegram-bot-token", default=os.environ.get("AW_HAYABUSA_TELEGRAM_BOT_TOKEN", ""))
    p.add_argument("--telegram-chat-ids", default=os.environ.get("AW_HAYABUSA_TELEGRAM_CHAT_IDS", ""))
    args = p.parse_args()

    intake_path = pathlib.Path(args.intake_json)
    intake = json.loads(intake_path.read_text(encoding="utf-8"))
    summary = analyze_report(pathlib.Path(intake["report_dir"]))
    case_api_base = args.case_api_base.rstrip("/")
    if case_api_base.endswith("/api/0/dlp/cases"):
        case_api_base = case_api_base[: -len("/api/0/dlp/cases")]
    case_id = args.case_id
    created_case = None
    case_error = None
    linked = False
    comment_added = False

    try:
        if case_id is None and args.auto_create and severity_meets(summary["severity"], args.auto_create_min_severity):
            created_case = post_json(f"{case_api_base}/api/0/dlp/cases", build_case_payload(intake, summary))
            case_id = int(created_case["id"])

        if case_id is not None:
            patch_json(
                f"{case_api_base}/api/0/dlp/cases/{case_id}",
                {"severity": summary["severity"]},
            )
            post_json(
                f"{case_api_base}/api/0/dlp/cases/{case_id}/forensics/hayabusa",
                build_hayabusa_payload(intake, args.mode, args.link_source),
            )
            linked = True
            post_json(
                f"{case_api_base}/api/0/dlp/cases/{case_id}/comments",
                {"comment": build_comment(summary, intake), "author": "aw-hayabusa-auto"},
            )
            comment_added = True
    except Exception as exc:  # noqa: BLE001
        case_error = str(exc)

    telegram_results: list[dict[str, Any]] = []
    if args.telegram_enabled and args.telegram_bot_token and severity_meets(summary["severity"], args.telegram_min_severity):
        chat_ids = [item.strip() for item in args.telegram_chat_ids.split(",") if item.strip()]
        if chat_ids:
            telegram_results = send_telegram(
                bot_token=args.telegram_bot_token,
                chat_ids=chat_ids,
                text=build_telegram_text(case_id, intake, summary),
            )

    result = {
        "summary": summary,
        "case_id": case_id,
        "case_created": created_case,
        "case_linked": linked,
        "case_comment_added": comment_added,
        "case_error": case_error,
        "telegram_results": telegram_results,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if case_error is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
