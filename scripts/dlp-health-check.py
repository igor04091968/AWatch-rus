#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from urllib import request


def _env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


def _http_json(url: str, timeout: int = 15, attempts: int = 2, backoff_seconds: float = 0.5) -> Any:
    last_exc: Exception | None = None
    for attempt in range(max(1, attempts)):
        try:
            with request.urlopen(url, timeout=timeout) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except Exception as exc:
            last_exc = exc
            if attempt + 1 >= max(1, attempts):
                break
            time.sleep(backoff_seconds * (2**attempt))
    assert last_exc is not None
    raise last_exc


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


def _int_or_zero(value: Any) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return 0


def _path_tail(value: Any, parts: int = 2) -> str:
    text = str(value or "").replace("\\", "/").strip()
    if not text:
        return ""
    tokens = [item for item in text.split("/") if item]
    return "/".join(tokens[-parts:]) if tokens else text


def _text_excerpt(value: Any, limit: int = 120) -> str:
    text = " ".join(str(value or "").split())
    if len(text) <= limit:
        return text
    return text[: max(0, limit - 1)].rstrip() + "…"


def _load_counter_state(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {"counters": {}}
    except Exception:
        return {"counters": {}}
    if not isinstance(payload, dict):
        return {"counters": {}}
    counters = payload.get("counters")
    if not isinstance(counters, dict):
        payload["counters"] = {}
    return payload


def _save_counter_state(path: Path, state: dict[str, Any]) -> str | None:
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = path.parent / f".{path.name}.{os.getpid()}.tmp"
        tmp_path.write_text(json.dumps(state, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        os.replace(tmp_path, path)
        return None
    except Exception as exc:
        try:
            path.write_text(json.dumps(state, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            return None
        except Exception:
            return str(exc)


def _counter_delta(counter_state: dict[str, Any] | None, key: str, current_value: int) -> tuple[int | None, int]:
    if counter_state is None:
        return None, current_value
    counters = counter_state.setdefault("counters", {})
    if not isinstance(counters, dict):
        counters = {}
        counter_state["counters"] = counters
    previous: int | None = None
    try:
        previous = int(counters[key])
    except (KeyError, TypeError, ValueError):
        previous = None
    counters[key] = current_value
    if previous is None or current_value < previous:
        return previous, 0
    return previous, current_value - previous


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


def check_incident_runtime(
    report: HealthReport,
    api_base: str,
    buckets: dict[str, Any],
    sample_limit: int = 20,
) -> None:
    now = _now_utc()
    prefix = "aw-dlp-incidents_"
    matched = sorted(bucket_id for bucket_id in buckets if bucket_id.startswith(prefix))
    if not matched:
        report.add("incident-runtime", "ok", "no incident buckets to sample", bucket_count=0)
        return

    if sample_limit <= 0:
        metadata = []
        for bucket_id in matched:
            ts = _latest_bucket_ts(api_base, bucket_id, buckets.get(bucket_id, {}))
            metadata.append(
                {
                    "bucket": bucket_id,
                    "end": ts.isoformat().replace("+00:00", "Z") if ts else None,
                    "age_seconds": _age_seconds(ts, now),
                }
            )
        report.add(
            "incident-runtime",
            "ok",
            "incident event sampling disabled",
            bucket_count=len(matched),
            sample_limit=sample_limit,
            metadata=metadata,
        )
        return

    sampled: list[dict[str, Any]] = []
    latest_incidents: list[dict[str, Any]] = []
    read_failed: list[dict[str, str]] = []
    totals = {
        "sampled_events": 0,
        "real_incidents": 0,
        "self_tests": 0,
    }
    severity_counts: dict[str, int] = {}
    action_counts: dict[str, int] = {}
    rule_counts: dict[str, int] = {}

    for bucket_id in matched:
        try:
            events = _http_json(f"{api_base}/buckets/{bucket_id}/events?limit={sample_limit}", timeout=5, attempts=1)
        except Exception as exc:
            read_failed.append({"bucket": bucket_id, "error": str(exc)})
            continue
        if not isinstance(events, list):
            read_failed.append({"bucket": bucket_id, "error": "events response is not a list"})
            continue

        bucket_summary = {
            "bucket": bucket_id,
            "sampled_events": len(events),
            "real_incidents": 0,
            "self_tests": 0,
        }
        sampled.append(bucket_summary)
        totals["sampled_events"] += len(events)

        for event in events:
            data = event.get("data") or {}
            if not isinstance(data, dict):
                continue

            signal_type = str(data.get("signalType") or "").strip()
            source = str(data.get("source") or "").strip()
            rule_id = str(data.get("ruleId") or data.get("rule_id") or "").strip()
            is_self_test = signal_type == "self_test" or source == "self-test" or rule_id.startswith("selftest-")
            if is_self_test:
                bucket_summary["self_tests"] += 1
                totals["self_tests"] += 1
                continue

            ts = _parse_ts(event.get("timestamp"))
            severity = str(data.get("severity") or "unknown").strip().lower() or "unknown"
            action = str(data.get("action") or "unknown").strip().lower() or "unknown"
            rule_key = rule_id or "unknown"
            severity_counts[severity] = severity_counts.get(severity, 0) + 1
            action_counts[action] = action_counts.get(action, 0) + 1
            rule_counts[rule_key] = rule_counts.get(rule_key, 0) + 1
            bucket_summary["real_incidents"] += 1
            totals["real_incidents"] += 1
            latest_incidents.append(
                {
                    "bucket": bucket_id,
                    "timestamp": event.get("timestamp"),
                    "age_seconds": _age_seconds(ts, now),
                    "ruleId": rule_id,
                    "severity": severity,
                    "action": action,
                    "username": str(data.get("username") or ""),
                    "hostname": str(data.get("hostname") or ""),
                    "source": source,
                    "message_excerpt": _text_excerpt(data.get("message")),
                }
            )

    latest_incidents.sort(key=lambda item: item.get("timestamp") or "", reverse=True)
    status = "ok"
    summary = f"{totals['real_incidents']} real incidents in sampled events"
    if read_failed:
        status = "warn"
        summary = f"{len(read_failed)} incident buckets failed to sample"
    elif totals["real_incidents"] == 0:
        summary = "no real incidents in sampled events"

    report.add(
        "incident-runtime",
        status,
        summary,
        bucket_count=len(matched),
        sample_limit=sample_limit,
        totals=totals,
        sampled=sampled,
        severity_counts=severity_counts,
        action_counts=action_counts,
        rule_counts=dict(sorted(rule_counts.items(), key=lambda item: (-item[1], item[0]))[:10]),
        latest_incidents=latest_incidents[:5],
        read_failed=read_failed,
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


def check_file_operations_runtime(
    report: HealthReport,
    api_base: str,
    buckets: dict[str, Any],
    sample_limit: int = 20,
    queue_warn_depth: int = 100,
    send_failure_warn_count: int = 1,
    counter_state: dict[str, Any] | None = None,
) -> None:
    now = _now_utc()
    prefix = "aw-file-operations_"
    matched = sorted(bucket_id for bucket_id in buckets if bucket_id.startswith(prefix))
    if not matched:
        report.add("file-operations-runtime", "warn", "no file-operations buckets to sample", bucket_count=0)
        return

    sampled: list[dict[str, Any]] = []
    latest_operations: list[dict[str, Any]] = []
    latest_health: list[dict[str, Any]] = []
    warnings: list[dict[str, Any]] = []
    read_failed: list[dict[str, str]] = []

    for bucket_id in matched:
        try:
            events = _http_json(f"{api_base}/buckets/{bucket_id}/events?limit={sample_limit}")
        except Exception as exc:
            read_failed.append({"bucket": bucket_id, "error": str(exc)})
            continue
        if not isinstance(events, list):
            read_failed.append({"bucket": bucket_id, "error": "events response is not a list"})
            continue

        operation_counts: dict[str, int] = {}
        latest_health_event: dict[str, Any] | None = None
        latest_health_ts: datetime | None = None

        for event in events:
            data = event.get("data") or {}
            if not isinstance(data, dict):
                continue
            ts = _parse_ts(event.get("timestamp"))
            signal_type = str(data.get("signalType") or "")
            operation = str(data.get("operation") or "")

            if signal_type == "collector_health":
                if latest_health_event is None or (ts is not None and (latest_health_ts is None or ts > latest_health_ts)):
                    latest_health_event = event
                    latest_health_ts = ts
                continue

            if operation:
                operation_counts[operation] = operation_counts.get(operation, 0) + 1
                latest_operations.append(
                    {
                        "bucket": bucket_id,
                        "timestamp": event.get("timestamp"),
                        "age_seconds": _age_seconds(ts, now),
                        "operation": operation,
                        "username": str(data.get("username") or ""),
                        "hostname": str(data.get("hostname") or ""),
                        "extension": str(data.get("extension") or ""),
                        "archiveHint": bool(data.get("archiveHint")),
                        "path_tail": _path_tail(data.get("path")),
                        "size": _int_or_zero(data.get("size")),
                    }
                )

        bucket_sample = {
            "bucket": bucket_id,
            "sampled_events": len(events),
            "operation_counts": operation_counts,
        }
        sampled.append(bucket_sample)

        if latest_health_event is None:
            warnings.append({"bucket": bucket_id, "metric": "collector_health", "value": "missing_in_sample"})
            continue

        health_data = latest_health_event.get("data") or {}
        send_failures = _int_or_zero(health_data.get("sendFailures"))
        previous_send_failures, send_failures_delta = _counter_delta(
            counter_state,
            f"file-operations:{bucket_id}:sendFailures",
            send_failures,
        )
        health_item = {
            "bucket": bucket_id,
            "timestamp": latest_health_event.get("timestamp"),
            "age_seconds": _age_seconds(latest_health_ts, now),
            "queueDepth": _int_or_zero(health_data.get("queueDepth")),
            "eventsEnqueued": _int_or_zero(health_data.get("eventsEnqueued")),
            "eventsFlushed": _int_or_zero(health_data.get("eventsFlushed")),
            "sendFailures": send_failures,
            "sendFailuresPrevious": previous_send_failures,
            "sendFailuresDelta": send_failures_delta,
            "username": str(health_data.get("username") or ""),
            "hostname": str(health_data.get("hostname") or ""),
            "sessionId": _int_or_zero(health_data.get("sessionId")),
        }
        latest_health.append(health_item)
        if health_item["queueDepth"] > queue_warn_depth:
            warnings.append({"bucket": bucket_id, "metric": "queueDepth", "value": health_item["queueDepth"], "threshold": queue_warn_depth})
        if send_failure_warn_count > 0 and health_item["sendFailuresDelta"] >= send_failure_warn_count:
            warnings.append(
                {
                    "bucket": bucket_id,
                    "metric": "sendFailuresDelta",
                    "value": health_item["sendFailuresDelta"],
                    "current": health_item["sendFailures"],
                    "previous": health_item["sendFailuresPrevious"],
                    "threshold": send_failure_warn_count,
                }
            )

    latest_operations.sort(key=lambda item: item.get("timestamp") or "", reverse=True)
    status = "ok"
    summary = f"{len(matched)} file-operations buckets sampled"
    if read_failed:
        status = "warn"
        summary = f"{len(read_failed)} file-operations buckets failed to sample"
    elif warnings:
        status = "warn"
        summary = "file-operations runtime counters outside expectations"

    report.add(
        "file-operations-runtime",
        status,
        summary,
        bucket_count=len(matched),
        sample_limit=sample_limit,
        sampled=sampled,
        latest_health=latest_health,
        latest_operations=latest_operations[:5],
        warnings=warnings,
        read_failed=read_failed,
        thresholds={
            "queueDepth": queue_warn_depth,
            "sendFailures": send_failure_warn_count,
        },
    )


def check_endpoint_signal_buckets(
    report: HealthReport,
    api_base: str,
    buckets: dict[str, Any],
    max_age_seconds: int,
) -> None:
    now = _now_utc()
    prefix = "aw-dlp-endpoint-signals_"
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
            "buckets:endpoint-signals",
            "ok",
            "no active managed hosts require endpoint-signals freshness",
            active_hosts=[],
            ignored_unmanaged=ignored_unmanaged,
            ignored_inactive=ignored_inactive,
            worktime_hosts=sorted(worktime),
        )
        return

    status = "ok"
    summary = f"{len(fresh)} active host buckets fresh"
    if missing_active:
        status = "fail"
        summary = f"{len(missing_active)} active hosts missing endpoint-signals buckets"
    elif stale:
        status = "fail"
        summary = f"{len(stale)} active host buckets stale"
    elif unknown:
        status = "warn"
        summary = f"{len(unknown)} active host buckets without timestamp"

    report.add(
        "buckets:endpoint-signals",
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


def check_endpoint_self_test_metrics(
    report: HealthReport,
    api_base: str,
    buckets: dict[str, Any],
    queue_warn_depth: int = 100,
    send_failure_warn_count: int = 1,
    counter_state: dict[str, Any] | None = None,
) -> None:
    now = _now_utc()
    missing: list[str] = []
    latest_self_tests: list[dict[str, Any]] = []
    warnings: list[dict[str, Any]] = []
    expected = ("queueDepth", "eventsEnqueued", "eventsFlushed", "sendFailures")
    for bucket_id in sorted(k for k in buckets if k.startswith("aw-dlp-endpoint-signals_")):
        try:
            events = _http_json(f"{api_base}/buckets/{bucket_id}/events?limit=20")
        except Exception as exc:
            report.add(f"endpoint-self-test:{bucket_id}", "warn", f"failed to read events: {exc}", bucket=bucket_id)
            continue
        latest_event: dict[str, Any] | None = None
        latest_ts: datetime | None = None
        if isinstance(events, list):
            for event in events:
                data = event.get("data") or {}
                if data.get("signalType") != "self_test" or not all(key in data for key in expected):
                    continue
                ts = _parse_ts(event.get("timestamp"))
                if latest_event is None or (ts is not None and (latest_ts is None or ts > latest_ts)):
                    latest_event = event
                    latest_ts = ts
        if latest_event is None:
            missing.append(bucket_id)
            continue

        data = latest_event.get("data") or {}
        send_failures = _int_or_zero(data.get("sendFailures"))
        previous_send_failures, send_failures_delta = _counter_delta(
            counter_state,
            f"endpoint-self-test:{bucket_id}:sendFailures",
            send_failures,
        )
        item = {
            "bucket": bucket_id,
            "timestamp": latest_event.get("timestamp"),
            "age_seconds": _age_seconds(latest_ts, now),
            "queueDepth": _int_or_zero(data.get("queueDepth")),
            "eventsEnqueued": _int_or_zero(data.get("eventsEnqueued")),
            "eventsFlushed": _int_or_zero(data.get("eventsFlushed")),
            "sendFailures": send_failures,
            "sendFailuresPrevious": previous_send_failures,
            "sendFailuresDelta": send_failures_delta,
        }
        latest_self_tests.append(item)
        if item["queueDepth"] > queue_warn_depth:
            warnings.append({"bucket": bucket_id, "metric": "queueDepth", "value": item["queueDepth"], "threshold": queue_warn_depth})
        if send_failure_warn_count > 0 and item["sendFailuresDelta"] >= send_failure_warn_count:
            warnings.append(
                {
                    "bucket": bucket_id,
                    "metric": "sendFailuresDelta",
                    "value": item["sendFailuresDelta"],
                    "current": item["sendFailures"],
                    "previous": item["sendFailuresPrevious"],
                    "threshold": send_failure_warn_count,
                }
            )

    if missing:
        report.add(
            "endpoint-self-test-metrics",
            "warn",
            "missing transport metrics in sampled self_test events",
            buckets=missing,
            latest_self_tests=latest_self_tests,
            thresholds={
                "queueDepth": queue_warn_depth,
                "sendFailures": send_failure_warn_count,
            },
        )
    elif warnings:
        report.add(
            "endpoint-self-test-metrics",
            "warn",
            "endpoint transport counters outside thresholds",
            latest_self_tests=latest_self_tests,
            warnings=warnings,
            thresholds={
                "queueDepth": queue_warn_depth,
                "sendFailures": send_failure_warn_count,
            },
        )
    else:
        report.add(
            "endpoint-self-test-metrics",
            "ok",
            "self_test transport metrics present",
            latest_self_tests=latest_self_tests,
            thresholds={
                "queueDepth": queue_warn_depth,
                "sendFailures": send_failure_warn_count,
            },
        )


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
    parser.add_argument("--endpoint-queue-warn-depth", type=int, default=int(_env("AW_DLP_HEALTH_ENDPOINT_QUEUE_WARN_DEPTH", "100")))
    parser.add_argument("--endpoint-send-failure-warn-count", type=int, default=int(_env("AW_DLP_HEALTH_ENDPOINT_SEND_FAILURE_WARN_COUNT", "1")))
    parser.add_argument("--fileops-sample-limit", type=int, default=int(_env("AW_DLP_HEALTH_FILEOPS_SAMPLE_LIMIT", "20")))
    parser.add_argument("--fileops-queue-warn-depth", type=int, default=int(_env("AW_DLP_HEALTH_FILEOPS_QUEUE_WARN_DEPTH", "100")))
    parser.add_argument("--fileops-send-failure-warn-count", type=int, default=int(_env("AW_DLP_HEALTH_FILEOPS_SEND_FAILURE_WARN_COUNT", "1")))
    parser.add_argument("--incident-sample-limit", type=int, default=int(_env("AW_DLP_HEALTH_INCIDENT_SAMPLE_LIMIT", "0")))
    parser.add_argument("--state-dir", default=_env("AW_DLP_HEALTH_STATE_DIR", "/var/lib/activitywatch/health"))
    parser.add_argument("--report-dir", default=_env("AW_DLP_COMPLIANCE_REPORT_DIR", "/opt/activitywatch/dlp-compliance/reports"))
    parser.add_argument("--profiles", default=_env("AW_DLP_COMPLIANCE_PROFILES", "152-fz,pci-dss"))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    report = HealthReport()
    aw_api_base = args.aw_server.rstrip("/") + "/api/0"
    counter_state_path = Path(args.state_dir) / "dlp-health-check-counters.json"
    counter_state = _load_counter_state(counter_state_path)

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
        check_endpoint_signal_buckets(report, aw_api_base, buckets, args.max_age_seconds)
        check_file_operations_buckets(report, aw_api_base, buckets, args.max_age_seconds, args.strict_fileops)
        check_file_operations_runtime(
            report,
            aw_api_base,
            buckets,
            args.fileops_sample_limit,
            args.fileops_queue_warn_depth,
            args.fileops_send_failure_warn_count,
            counter_state,
        )
        check_incident_buckets(report, aw_api_base, buckets, args.max_age_seconds * 24)
        check_incident_runtime(report, aw_api_base, buckets, args.incident_sample_limit)
        check_endpoint_self_test_metrics(
            report,
            aw_api_base,
            buckets,
            args.endpoint_queue_warn_depth,
            args.endpoint_send_failure_warn_count,
            counter_state,
        )
    except Exception as exc:
        report.add("aw:buckets-index", "fail", f"failed to inspect bucket index: {exc}")

    state_error = _save_counter_state(counter_state_path, counter_state)
    if state_error:
        report.add("state:counters", "warn", f"failed to save counter baseline: {state_error}", path=str(counter_state_path))

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
