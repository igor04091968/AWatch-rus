#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from urllib import request


ENV_FILE = Path("/etc/activitywatch/aw-server.env")


def load_env_file(path: Path) -> None:
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip("'").strip('"')
        os.environ.setdefault(key, value)


def env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


def now_utc() -> datetime:
    return datetime.now(UTC)


def parse_ts(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(UTC)
    except ValueError:
        return None


def age_seconds(ts: datetime | None, now: datetime) -> int | None:
    if ts is None:
        return None
    return max(0, int((now - ts).total_seconds()))


def http_json(url: str, timeout: int = 10) -> Any:
    with request.urlopen(url, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def run_command(cmd: list[str]) -> tuple[int, str]:
    proc = subprocess.run(
        cmd,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    return proc.returncode, proc.stdout.strip()


def tcp_connect(host: str, port: int, timeout: float) -> tuple[bool, str]:
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True, "connected"
    except OSError as exc:
        return False, str(exc)


@dataclass
class CheckResult:
    name: str
    status: str
    summary: str
    details: dict[str, Any]


class Report:
    def __init__(self) -> None:
        self.results: list[CheckResult] = []

    def add(self, name: str, status: str, summary: str, **details: Any) -> None:
        self.results.append(CheckResult(name=name, status=status, summary=summary, details=details))

    @property
    def ok(self) -> bool:
        return not any(item.status == "fail" for item in self.results)

    def as_dict(self) -> dict[str, Any]:
        counts = {"ok": 0, "warn": 0, "fail": 0}
        for item in self.results:
            counts[item.status] = counts.get(item.status, 0) + 1
        return {
            "generated_at_utc": now_utc().isoformat().replace("+00:00", "Z"),
            "ok": self.ok,
            "counts": counts,
            "results": [
                {
                    "name": item.name,
                    "status": item.status,
                    "summary": item.summary,
                    "details": item.details,
                }
                for item in self.results
            ],
        }

    def render_text(self) -> str:
        icon = {"ok": "OK", "warn": "WARN", "fail": "FAIL"}
        lines = ["=== AW-RUS Health ===", f"Timestamp: {now_utc().isoformat().replace('+00:00', 'Z')}", ""]
        for item in self.results:
            lines.append(f"[{icon.get(item.status, item.status.upper())}] {item.name}: {item.summary}")
        lines.append("")
        payload = self.as_dict()
        lines.append(
            "Counts: ok={ok} warn={warn} fail={fail}".format(
                ok=payload["counts"]["ok"],
                warn=payload["counts"]["warn"],
                fail=payload["counts"]["fail"],
            )
        )
        lines.append(f"Overall: {'OK' if payload['ok'] else 'FAIL'}")
        return "\n".join(lines)


def latest_bucket_event(api_base: str, bucket_id: str) -> dict[str, Any] | None:
    events = http_json(f"{api_base}/buckets/{bucket_id}/events?limit=20")
    if isinstance(events, list) and events:
        events = [item for item in events if isinstance(item, dict)]
        if not events:
            return None
        events.sort(key=lambda item: item.get("timestamp") or "", reverse=True)
        return events[0]
    return None


def host_activity_from_worktime(event: dict[str, Any] | None, max_age_seconds: int) -> dict[str, Any]:
    now = now_utc()
    if not event:
        return {"fresh": False, "active": False, "age_seconds": None, "timestamp": None}
    ts = parse_ts(event.get("timestamp"))
    age = age_seconds(ts, now)
    data = event.get("data") or {}
    is_fresh = age is not None and age <= max_age_seconds
    is_active = bool(is_fresh and data.get("active"))
    return {
        "fresh": bool(is_fresh),
        "active": bool(is_active),
        "age_seconds": age,
        "timestamp": event.get("timestamp"),
        "data": data,
    }


def bucket_health(
    api_base: str,
    bucket_id: str,
    max_age_seconds: int,
    missing_status: str,
    stale_status: str,
) -> tuple[str, str, dict[str, Any]]:
    try:
        event = latest_bucket_event(api_base, bucket_id)
    except Exception as exc:
        return "fail", f"bucket query failed: {exc}", {"bucket": bucket_id}

    if not event:
        return missing_status, "no events", {"bucket": bucket_id}

    ts = parse_ts(event.get("timestamp"))
    age = age_seconds(ts, now_utc())
    details = {"bucket": bucket_id, "timestamp": event.get("timestamp"), "age_seconds": age}
    if age is None:
        return "warn", "timestamp parse failed", details
    if age > max_age_seconds:
        return stale_status, f"stale ({age}s)", details
    return "ok", f"fresh ({age}s)", details


def latest_validation_report(validation_dir: Path) -> Path | None:
    candidates = sorted(
        (path for path in validation_dir.glob("*-aw_validate_ansible.json") if path.is_file()),
        key=lambda item: item.stat().st_mtime,
        reverse=True,
    )
    return candidates[0] if candidates else None


def write_atomic(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as handle:
        handle.write(content)
        tmp_name = handle.name
    os.replace(tmp_name, path)


def check_wrapper(report: Report, name: str, cmd: list[str], json_mode: bool = False) -> None:
    if not Path(cmd[0]).exists():
        report.add(name, "warn", "binary missing", command=cmd)
        return
    rc, output = run_command(cmd)
    details: dict[str, Any] = {"command": cmd, "returncode": rc}
    if json_mode:
        try:
            details["payload"] = json.loads(output) if output else {}
        except json.JSONDecodeError:
            details["raw_output"] = output
            report.add(name, "fail", "invalid JSON output", **details)
            return
    else:
        details["output"] = output
    report.add(name, "ok" if rc == 0 else "fail", "passed" if rc == 0 else "failed", **details)


def main() -> int:
    load_env_file(ENV_FILE)

    parser = argparse.ArgumentParser(description="Unified AW-RUS health orchestrator")
    parser.add_argument("--aw-server", default=env("AW_SERVER_URL", "http://127.0.0.1:5600"))
    parser.add_argument("--worktime-api", default=env("AW_WORKTIME_REPORT_BASE", "http://127.0.0.1:5610"))
    parser.add_argument("--rdp-host", default=env("AW_MONITORED_WINDOWS_HOST", "192.168.100.18"))
    parser.add_argument("--rdp-hostname", default=env("AW_MONITORED_WINDOWS_HOSTNAME", "SHARKON2025"))
    parser.add_argument("--state-dir", default=env("AW_RUS_HEALTH_STATE_DIR", "/var/lib/activitywatch/health"))
    parser.add_argument("--validation-dir", default=env("AW_RUS_HEALTH_VALIDATION_DIR", "/var/lib/activitywatch/health/windows-validation"))
    parser.add_argument("--session-max-age-seconds", type=int, default=int(env("AW_RUS_HEALTH_SESSION_MAX_AGE_SECONDS", "900")))
    parser.add_argument("--interactive-max-age-seconds", type=int, default=int(env("AW_RUS_HEALTH_INTERACTIVE_MAX_AGE_SECONDS", "900")))
    parser.add_argument("--session-events-max-age-seconds", type=int, default=int(env("AW_RUS_HEALTH_SESSION_EVENTS_MAX_AGE_SECONDS", "604800")))
    parser.add_argument("--validation-max-age-seconds", type=int, default=int(env("AW_RUS_HEALTH_VALIDATION_MAX_AGE_SECONDS", "259200")))
    parser.add_argument("--tcp-timeout-seconds", type=float, default=float(env("AW_RUS_HEALTH_TCP_TIMEOUT_SECONDS", "3")))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    report = Report()
    aw_api_base = args.aw_server.rstrip("/")
    if not aw_api_base.endswith("/api/0"):
        aw_api_base = aw_api_base.rstrip("/") + "/api/0"

    check_wrapper(report, "wrapper:aw-health-check", ["/usr/local/bin/aw-health-check"])
    check_wrapper(report, "wrapper:dlp-health-check", ["/usr/local/bin/dlp-health-check", "--json"], json_mode=True)

    try:
        info = http_json(f"{aw_api_base}/info")
        report.add("http:aw-server", "ok", "activitywatch API responded", version=info.get("version"))
    except Exception as exc:
        report.add("http:aw-server", "fail", f"activitywatch API failed: {exc}", url=f"{aw_api_base}/info")

    try:
        payload = http_json(args.worktime_api.rstrip("/") + "/reports/worktime/today")
        rows = len(payload) if isinstance(payload, list) else None
        report.add("http:worktime-api", "ok", "worktime API responded", rows=rows)
    except Exception as exc:
        report.add("http:worktime-api", "fail", f"worktime API failed: {exc}", url=args.worktime_api)

    for port, label in ((5985, "winrm"), (3389, "rdp")):
        ok, message = tcp_connect(args.rdp_host, port, args.tcp_timeout_seconds)
        report.add(f"tcp:{label}", "ok" if ok else "fail", message if ok else f"unreachable: {message}", host=args.rdp_host, port=port)

    try:
        buckets = http_json(f"{aw_api_base}/buckets")
        if not isinstance(buckets, dict):
            raise RuntimeError("bucket index is not a dict")
        report.add("aw:buckets-index", "ok", "bucket index loaded", total=len(buckets))
    except Exception as exc:
        report.add("aw:buckets-index", "fail", f"failed to load bucket index: {exc}")
        buckets = {}

    host = args.rdp_hostname
    worktime_bucket = f"aw-worktime-sessions_{host}"
    worktime_event = None
    if buckets:
        try:
            worktime_event = latest_bucket_event(aw_api_base, worktime_bucket)
        except Exception:
            worktime_event = None
    activity = host_activity_from_worktime(worktime_event, args.session_max_age_seconds)
    if worktime_event:
        status, summary, details = bucket_health(
            aw_api_base,
            worktime_bucket,
            args.session_max_age_seconds,
            missing_status="fail",
            stale_status="fail",
        )
        details["host_activity"] = activity
        report.add("bucket:worktime-sessions", status, summary, **details)
    else:
        report.add("bucket:worktime-sessions", "fail", "no events", bucket=worktime_bucket, host_activity=activity)

    interactive_required = bool(activity["active"])
    for bucket_name, label in (
        ("aw-watcher-afk", "bucket:afk"),
        ("aw-watcher-window", "bucket:window"),
        ("aw-dlp-endpoint-signals", "bucket:endpoint-signals"),
    ):
        status, summary, details = bucket_health(
            aw_api_base,
            f"{bucket_name}_{host}",
            args.interactive_max_age_seconds,
            missing_status="fail" if interactive_required else "warn",
            stale_status="fail" if interactive_required else "warn",
        )
        details["interactive_required"] = interactive_required
        details["host_activity"] = activity
        report.add(label, status, summary, **details)

    session_status, session_summary, session_details = bucket_health(
        aw_api_base,
        f"aw-session-events_{host}",
        args.session_events_max_age_seconds,
        missing_status="fail",
        stale_status="warn",
    )
    report.add("bucket:session-events", session_status, session_summary, **session_details)

    validation_dir = Path(args.validation_dir)
    validation_report = latest_validation_report(validation_dir)
    if validation_report is None:
        report.add("validation:windows", "warn", "no validation report snapshot", directory=str(validation_dir))
    else:
        try:
            payload = json.loads(validation_report.read_text(encoding="utf-8-sig"))
            age = age_seconds(datetime.fromtimestamp(validation_report.stat().st_mtime, tz=UTC), now_utc())
            if age is not None and age > args.validation_max_age_seconds:
                report.add(
                    "validation:windows",
                    "warn",
                    f"validation snapshot is stale ({age}s)",
                    path=str(validation_report),
                    overall_ok=payload.get("overallOk"),
                    failed_sections=payload.get("summary", {}).get("failedSections", []),
                )
            elif payload.get("overallOk") is True:
                report.add("validation:windows", "ok", "validation snapshot OK", path=str(validation_report), age_seconds=age)
            else:
                report.add(
                    "validation:windows",
                    "fail",
                    "validation snapshot reports failure",
                    path=str(validation_report),
                    age_seconds=age,
                    failed_sections=payload.get("summary", {}).get("failedSections", []),
                )
        except Exception as exc:
            report.add("validation:windows", "fail", f"invalid validation snapshot: {exc}", path=str(validation_report))

    payload = report.as_dict()
    state_dir = Path(args.state_dir)
    write_atomic(state_dir / "aw-rus-health.json", json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
    write_atomic(state_dir / "aw-rus-health.txt", report.render_text() + "\n")

    if args.json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    else:
        print(report.render_text())
    return 0 if payload["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
