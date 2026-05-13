#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from urllib import request


def _env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


def _http_json(url: str, timeout: int = 10) -> Any:
    with request.urlopen(url, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _parse_ts(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(UTC)
    except ValueError:
        return None


def _now_utc() -> datetime:
    return datetime.now(UTC)


def _age_seconds(ts: datetime | None, now: datetime) -> int | None:
    if ts is None:
        return None
    return max(0, int((now - ts).total_seconds()))


def _run_systemctl(*args: str) -> tuple[int, str]:
    proc = subprocess.run(
        ["systemctl", *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    return proc.returncode, proc.stdout.strip()


@dataclass
class CheckResult:
    name: str
    status: str
    summary: str
    details: dict[str, Any]


class HealthReport:
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
        lines = ["=== DLP Health Check ===", f"Timestamp: {_now_utc().isoformat().replace('+00:00', 'Z')}", ""]
        for item in self.results:
            lines.append(f"[{icon.get(item.status, item.status.upper())}] {item.name}: {item.summary}")
            if item.details:
                lines.append(f"  details: {json.dumps(item.details, ensure_ascii=False, sort_keys=True)}")
        lines.append("")
        lines.append(f"Overall: {'OK' if self.ok else 'FAIL'}")
        return "\n".join(lines)


def check_http_endpoint(report: HealthReport, name: str, url: str) -> None:
    try:
        payload = _http_json(url)
        report.add(name, "ok", f"HTTP endpoint responded", url=url, payload=payload)
    except Exception as exc:
        report.add(name, "fail", f"HTTP endpoint failed: {exc}", url=url)


def check_systemd_unit(report: HealthReport, unit: str, kind: str) -> None:
    active_rc, active_out = _run_systemctl("is-active", unit)
    enabled_rc, enabled_out = _run_systemctl("is-enabled", unit)
    exists_rc, _ = _run_systemctl("status", unit)
    if exists_rc != 0 and active_rc != 0 and enabled_rc != 0:
        report.add(f"systemd:{unit}", "warn", "unit not installed", kind=kind)
        return

    if active_rc == 0 and enabled_rc == 0:
        report.add(f"systemd:{unit}", "ok", "active and enabled", kind=kind)
        return

    report.add(
        f"systemd:{unit}",
        "fail",
        "unit is not active/enabled",
        kind=kind,
        active=active_out or str(active_rc),
        enabled=enabled_out or str(enabled_rc),
    )


def _latest_bucket_ts(api_base: str, bucket_id: str, bucket_meta: dict[str, Any]) -> datetime | None:
    meta = bucket_meta.get("metadata") or {}
    ts = _parse_ts(meta.get("end"))
    if ts is not None:
        return ts
    try:
        events = _http_json(f"{api_base}/buckets/{bucket_id}/events?limit=1")
    except Exception:
        return None
    if isinstance(events, list) and events:
        return _parse_ts(events[0].get("timestamp"))
    return None


def _bucket_suffix(bucket_id: str, prefix: str) -> str:
    return bucket_id[len(prefix):] if bucket_id.startswith(prefix) else bucket_id


def check_bucket_group(
    report: HealthReport,
    api_base: str,
    buckets: dict[str, Any],
    name: str,
    prefix: str,
    max_age_seconds: int,
    severity_if_missing: str = "fail",
    severity_if_stale: str = "fail",
) -> None:
    now = _now_utc()
    matched = sorted(bucket_id for bucket_id in buckets if bucket_id.startswith(prefix))
    if not matched:
        report.add(
            f"buckets:{name}",
            severity_if_missing,
            f"no buckets matched prefix {prefix}",
            prefix=prefix,
        )
        return

    stale: list[dict[str, Any]] = []
    unknown: list[str] = []
    ages: dict[str, int] = {}
    for bucket_id in matched:
        ts = _latest_bucket_ts(api_base, bucket_id, buckets.get(bucket_id, {}))
        age = _age_seconds(ts, now)
        if age is None:
            unknown.append(bucket_id)
            continue
        ages[bucket_id] = age
        if age > max_age_seconds:
            stale.append({"bucket": bucket_id, "age_seconds": age})

    status = "ok"
    summary = f"{len(matched)} buckets, freshest ok"
    if stale:
        status = severity_if_stale
        summary = f"{len(stale)} stale buckets"
    elif unknown:
        status = "warn"
        summary = f"{len(unknown)} buckets without timestamp"

    report.add(
        f"buckets:{name}",
        status,
        summary,
        prefix=prefix,
        max_age_seconds=max_age_seconds,
        bucket_count=len(matched),
        max_observed_age_seconds=max(ages.values()) if ages else None,
        stale=stale,
        unknown=unknown,
    )


def check_incident_buckets(
    report: HealthReport,
    api_base: str,
    buckets: dict[str, Any],
    max_age_seconds: int,
) -> None:
    now = _now_utc()
    prefix = "aw-dlp-incidents_"
    matched = sorted(bucket_id for bucket_id in buckets if bucket_id.startswith(prefix))

    if not matched:
        report.add(
            "buckets:incidents",
            "ok",
            "no incident buckets yet",
            prefix=prefix,
            bucket_count=0,
        )
        return

    ages: dict[str, int] = {}
    unknown: list[str] = []
    stale: list[dict[str, Any]] = []
    for bucket_id in matched:
        ts = _latest_bucket_ts(api_base, bucket_id, buckets.get(bucket_id, {}))
        age = _age_seconds(ts, now)
        if age is None:
            unknown.append(bucket_id)
            continue
        ages[bucket_id] = age
        if age > max_age_seconds:
            stale.append({"bucket": bucket_id, "age_seconds": age})

    if stale and not unknown:
        report.add(
            "buckets:incidents",
            "ok",
            "no recent incidents",
            prefix=prefix,
            bucket_count=len(matched),
            max_age_seconds=max_age_seconds,
            max_observed_age_seconds=max(ages.values()) if ages else None,
            stale=stale,
            unknown=[],
        )
        return

    status = "ok" if not unknown else "warn"
    summary = "incident buckets healthy" if not unknown else f"{len(unknown)} incident buckets without timestamp"
    report.add(
        "buckets:incidents",
        status,
        summary,
        prefix=prefix,
        bucket_count=len(matched),
        max_age_seconds=max_age_seconds,
        max_observed_age_seconds=max(ages.values()) if ages else None,
        stale=stale,
        unknown=unknown,
    )


def _worktime_activity_map(api_base: str, buckets: dict[str, Any], max_age_seconds: int) -> dict[str, dict[str, Any]]:
    now = _now_utc()
    activity: dict[str, dict[str, Any]] = {}
    prefix = "aw-worktime-sessions_"
    for bucket_id in sorted(key for key in buckets if key.startswith(prefix)):
        host = _bucket_suffix(bucket_id, prefix)
        latest_ts: datetime | None = None
        latest_active = False
        try:
            events = _http_json(f"{api_base}/buckets/{bucket_id}/events?limit=20")
        except Exception:
            activity[host] = {"active": False, "age_seconds": None, "bucket": bucket_id}
            continue
        if isinstance(events, list):
            for event in events:
                ts = _parse_ts(event.get("timestamp"))
                if ts is None:
                    continue
                if latest_ts is None or ts > latest_ts:
                    latest_ts = ts
                    latest_active = bool((event.get("data") or {}).get("active"))
        activity[host] = {
            "active": bool(latest_ts and latest_active and (_age_seconds(latest_ts, now) or 0) <= max_age_seconds),
            "age_seconds": _age_seconds(latest_ts, now),
            "bucket": bucket_id,
        }
    return activity


def check_file_operations_buckets(
    report: HealthReport,
    api_base: str,
    buckets: dict[str, Any],
    max_age_seconds: int,
    strict: bool,
) -> None:
    now = _now_utc()
    prefix = "aw-file-operations_"
    matched = sorted(bucket_id for bucket_id in buckets if bucket_id.startswith(prefix))
    worktime = _worktime_activity_map(api_base, buckets, max_age_seconds)
    active_hosts = sorted(host for host, meta in worktime.items() if meta.get("active"))
    matched_by_host = {_bucket_suffix(bucket_id, prefix): bucket_id for bucket_id in matched}

    ignored_unmanaged: list[str] = []
    ignored_inactive: list[str] = []
    missing_active: list[str] = []
    stale: list[dict[str, Any]] = []
    unknown: list[str] = []
    fresh: list[str] = []

    for host, bucket_id in matched_by_host.items():
        if host not in worktime:
            ignored_unmanaged.append(bucket_id)
            continue
        if host not in active_hosts:
            ignored_inactive.append(bucket_id)
            continue
        ts = _latest_bucket_ts(api_base, bucket_id, buckets.get(bucket_id, {}))
        age = _age_seconds(ts, now)
        if age is None:
            unknown.append(bucket_id)
            continue
        if age > max_age_seconds:
            stale.append({"bucket": bucket_id, "age_seconds": age})
        else:
            fresh.append(bucket_id)

    for host in active_hosts:
        if host not in matched_by_host:
            missing_active.append(host)

    if not active_hosts:
        report.add(
            "buckets:file-operations",
            "ok",
            "no active managed hosts require file-operations freshness",
            active_hosts=[],
            ignored_unmanaged=ignored_unmanaged,
            ignored_inactive=ignored_inactive,
            worktime_hosts=sorted(worktime),
        )
        return

    status = "ok"
    summary = f"{len(fresh)} active host buckets fresh"
    if missing_active:
        status = "fail" if strict else "warn"
        summary = f"{len(missing_active)} active hosts missing file-operations buckets"
    elif stale:
        status = "fail" if strict else "warn"
        summary = f"{len(stale)} active host buckets stale"
    elif unknown:
        status = "warn"
        summary = f"{len(unknown)} active host buckets without timestamp"

    report.add(
        "buckets:file-operations",
        status,
        summary,
        active_hosts=active_hosts,
        fresh=fresh,
        stale=stale,
        missing_active=missing_active,
        unknown=unknown,
        ignored_unmanaged=ignored_unmanaged,
        ignored_inactive=ignored_inactive,
    )


def check_endpoint_self_test_metrics(report: HealthReport, api_base: str, buckets: dict[str, Any]) -> None:
    missing: list[str] = []
    expected = ("queueDepth", "eventsEnqueued", "eventsFlushed", "sendFailures")
    for bucket_id in sorted(k for k in buckets if k.startswith("aw-dlp-endpoint-signals_")):
        try:
            events = _http_json(f"{api_base}/buckets/{bucket_id}/events?limit=20")
        except Exception as exc:
            report.add(f"endpoint-self-test:{bucket_id}", "warn", f"failed to read events: {exc}", bucket=bucket_id)
            continue
        found = False
        if isinstance(events, list):
            for event in events:
                data = event.get("data") or {}
                if data.get("signalType") == "self_test" and all(key in data for key in expected):
                    found = True
                    break
        if not found:
            missing.append(bucket_id)

    if missing:
        report.add("endpoint-self-test-metrics", "warn", "missing transport metrics in recent self_test events", buckets=missing)
    else:
        report.add("endpoint-self-test-metrics", "ok", "recent self_test metrics present")


def check_compliance_reports(report: HealthReport, report_dir: Path, profiles: list[str], month: str) -> None:
    missing: list[str] = []
    present: list[str] = []
    for profile in profiles:
        for suffix in ("html", "json"):
            path = report_dir / f"{profile}-{month}.{suffix}"
            if path.exists():
                present.append(str(path))
            else:
                missing.append(str(path))
    if missing:
        report.add("compliance-reports", "fail", "missing expected compliance report artifacts", present=present, missing=missing)
    else:
        report.add("compliance-reports", "ok", "all expected compliance artifacts exist", present=present)


def main() -> int:
    parser = argparse.ArgumentParser(description="AWatch DLP health check")
    parser.add_argument("--aw-server", default=_env("AW_HEALTH_AW_SERVER", "http://127.0.0.1:5600"))
    parser.add_argument("--policy-server", default=_env("AW_HEALTH_POLICY_SERVER", "http://127.0.0.1:5601"))
    parser.add_argument("--case-server", default=_env("AW_HEALTH_CASE_SERVER", "http://127.0.0.1:5602"))
    parser.add_argument("--max-age-seconds", type=int, default=int(_env("AW_HEALTH_MAX_AGE_SECONDS", "900")))
    parser.add_argument("--strict-fileops", action="store_true", default=_env("AW_HEALTH_STRICT_FILEOPS", "0").lower() in {"1", "true", "yes", "on"})
    parser.add_argument("--report-dir", default=_env("AW_DLP_COMPLIANCE_REPORT_DIR", "/opt/activitywatch/dlp-compliance/reports"))
    parser.add_argument("--profiles", default=_env("AW_DLP_COMPLIANCE_PROFILES", "152-fz,pci-dss"))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    report = HealthReport()
    aw_api_base = args.aw_server.rstrip("/") + "/api/0"

    check_http_endpoint(report, "http:aw", f"{aw_api_base}/info")
    check_http_endpoint(report, "http:policy", args.policy_server.rstrip("/") + "/healthz")
    check_http_endpoint(report, "http:cases", args.case_server.rstrip("/") + "/health")

    for unit in (
        "activitywatch-server",
        "aw-dlp-policy-engine.service",
        "aw-dlp-case-management.service",
        "aw-worktime-api.service",
    ):
        check_systemd_unit(report, unit, "service")

    for unit in (
        "aw-dlp-report-scheduler.timer",
        "aw-dlp-syslog-forwarder.timer",
        "aw-dlp-webhook-sender.timer",
        "aw-dlp-cef-exporter.timer",
        "activitywatch-dlp-aggregator.timer",
        "aw-dlp-ioc-refresh.timer",
        "aw-worktime-ui-bridge.timer",
    ):
        check_systemd_unit(report, unit, "timer")

    try:
        buckets = _http_json(f"{aw_api_base}/buckets")
        if not isinstance(buckets, dict):
            raise RuntimeError("bucket list is not a dict")
        report.add("aw:buckets-index", "ok", "bucket index loaded", total=len(buckets))
        check_bucket_group(report, aw_api_base, buckets, "endpoint-signals", "aw-dlp-endpoint-signals_", args.max_age_seconds)
        check_file_operations_buckets(report, aw_api_base, buckets, args.max_age_seconds, args.strict_fileops)
        check_incident_buckets(report, aw_api_base, buckets, args.max_age_seconds * 24)
        check_endpoint_self_test_metrics(report, aw_api_base, buckets)
    except Exception as exc:
        report.add("aw:buckets-index", "fail", f"failed to inspect bucket index: {exc}")

    month = _now_utc().strftime("%Y-%m")
    profiles = [x.strip() for x in args.profiles.split(",") if x.strip()]
    check_compliance_reports(report, Path(args.report_dir), profiles, month)

    payload = report.as_dict()
    if args.json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    else:
        print(report.render_text())
    return 0 if payload["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
