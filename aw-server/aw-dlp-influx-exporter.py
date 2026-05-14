#!/usr/bin/env python3
import json
import os
import sys
import urllib.parse
import urllib.request
from datetime import UTC, datetime, timedelta


def _env_bool(name: str, default: bool = False) -> bool:
    value = os.environ.get(name, "").strip().lower()
    if not value:
        return default
    return value in {"1", "true", "yes", "on"}


AW_API_BASE = os.environ.get("AW_DLP_AW_API_BASE", "http://127.0.0.1:5600/api/0").strip().rstrip("/")
CASE_API_BASE = os.environ.get("AW_DLP_CASE_API_BASE", "http://127.0.0.1:5602/api/0/dlp/cases").strip().rstrip("/")
INFLUX_URL = os.environ.get("AW_DLP_INFLUX_URL", "").strip().rstrip("/")
INFLUX_ORG = os.environ.get("AW_DLP_INFLUX_ORG", "proxmox").strip() or "proxmox"
INFLUX_BUCKET = os.environ.get("AW_DLP_INFLUX_BUCKET", "aw_metrics").strip() or "aw_metrics"
INFLUX_TOKEN = os.environ.get("AW_DLP_INFLUX_TOKEN", "").strip()
INFLUX_ENABLED = _env_bool("AW_DLP_INFLUX_ENABLED", False)
HOSTS = [item.strip() for item in os.environ.get("AW_DLP_INFLUX_HOSTS", "SHARKON2025").split(",") if item.strip()]
LOOKBACK_DAYS = int(os.environ.get("AW_DLP_INFLUX_LOOKBACK_DAYS", "30") or "30")
EVENT_LIMIT = int(os.environ.get("AW_DLP_INFLUX_EVENT_LIMIT", "2000") or "2000")
CASE_LIMIT = int(os.environ.get("AW_DLP_CASE_LIMIT", "500") or "500")


def utc_now() -> datetime:
    return datetime.now(tz=UTC)


def pts(value: str | None) -> datetime:
    if not value:
        return utc_now()
    normalized = value.replace("Z", "+00:00")
    parsed = datetime.fromisoformat(normalized)
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=UTC)
    return parsed.astimezone(UTC)


def _escape_tag(value: object) -> str:
    return (
        str(value or "")
        .replace("\\", "\\\\")
        .replace(" ", "\\ ")
        .replace(",", "\\,")
        .replace("=", "\\=")
    )


def _line(measurement: str, tags: dict[str, object], fields: dict[str, object], timestamp_ns: int) -> str:
    tag_part = ",".join(f"{key}={_escape_tag(value)}" for key, value in sorted(tags.items()) if value not in (None, ""))
    field_parts: list[str] = []
    for key, value in fields.items():
        if value is None:
            continue
        if isinstance(value, bool):
            field_parts.append(f"{key}={'true' if value else 'false'}")
        elif isinstance(value, int):
            field_parts.append(f"{key}={value}i")
        elif isinstance(value, float):
            field_parts.append(f"{key}={value}")
        else:
            text = str(value).replace("\\", "\\\\").replace('"', '\\"')
            field_parts.append(f'{key}="{text}"')
    if not field_parts:
        return ""
    if tag_part:
        return f"{measurement},{tag_part} {','.join(field_parts)} {timestamp_ns}"
    return f"{measurement} {','.join(field_parts)} {timestamp_ns}"


def _timestamp_ns(dt: datetime) -> int:
    return int(dt.astimezone(UTC).timestamp() * 1_000_000_000)


def _get_json(url: str) -> object:
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.loads(response.read().decode("utf-8"))


def fetch_bucket_events(bucket_id: str, start: datetime, end: datetime, limit: int) -> list[dict]:
    query = urllib.parse.urlencode(
        {
            "start": start.astimezone(UTC).isoformat().replace("+00:00", "Z"),
            "end": end.astimezone(UTC).isoformat().replace("+00:00", "Z"),
            "limit": str(limit),
        }
    )
    url = f"{AW_API_BASE}/buckets/{urllib.parse.quote(bucket_id, safe='')}/events?{query}"
    payload = _get_json(url)
    return payload if isinstance(payload, list) else []


def fetch_cases(host: str) -> list[dict]:
    query = urllib.parse.urlencode({"host": host, "limit": str(CASE_LIMIT)})
    payload = _get_json(f"{CASE_API_BASE}?{query}")
    return payload if isinstance(payload, list) else []


def _s(value: object) -> str:
    return str(value or "").strip()


def _first_nonempty(*values: object, default: str = "") -> str:
    for value in values:
        text = _s(value)
        if text:
            return text
    return default


def normalize_incident(event: dict, default_host: str) -> dict[str, object]:
    data = event.get("data") or {}
    source_event = data.get("sourceEvent") or {}
    source_data = source_event.get("data") or {}
    nested = data.get("incident") or {}

    signal_type = _first_nonempty(data.get("signalType"), source_data.get("signalType"), default="unknown")
    username = _first_nonempty(
        data.get("username"),
        source_data.get("username"),
        source_data.get("owner"),
        source_data.get("host"),
        default="unknown",
    )
    host = _first_nonempty(data.get("hostname"), data.get("host"), source_data.get("hostname"), default=default_host)
    severity = _first_nonempty(data.get("severity"), nested.get("severity"), default="unknown")
    action = _first_nonempty(data.get("action"), nested.get("verdict"), default="incident")
    message = _first_nonempty(
        data.get("message"),
        source_data.get("documentName"),
        source_data.get("documentNameOriginal"),
        nested.get("comment"),
    )
    return {
        "host": host,
        "signal_type": signal_type,
        "username": username,
        "severity": severity,
        "action": action,
        "message": message,
        "rule_id": _first_nonempty(data.get("ruleId")),
        "source": _first_nonempty(data.get("source"), source_data.get("source"), data.get("sourceBucket")),
        "document_name": _first_nonempty(source_data.get("documentName"), source_data.get("documentNameOriginal")),
        "printer_name": _first_nonempty(source_data.get("printerName")),
        "incident_status": _first_nonempty(nested.get("status")),
        "incident_verdict": _first_nonempty(nested.get("verdict")),
        "regex_matches": len(data.get("regexMatches") or []),
        "dictionary_matches": len(data.get("dictionaryMatches") or []),
        "ocr_requested": bool(data.get("ocrRequested")),
    }


def build_endpoint_lines(host: str, events: list[dict]) -> list[str]:
    lines: list[str] = []
    for item in events:
        data = item.get("data") or {}
        signal_type = _first_nonempty(data.get("signalType"), default="unknown")
        timestamp_ns = _timestamp_ns(pts(item.get("timestamp")))
        event_id = item.get("id") or f"{signal_type}-{timestamp_ns}"
        username = _first_nonempty(data.get("username"), data.get("owner"), default="unknown")
        if signal_type == "self_test":
            lines.append(
                _line(
                    "aw_dlp_endpoint_self_test",
                    {
                        "host": _first_nonempty(data.get("hostname"), default=host),
                        "event_id": event_id,
                        "username": username,
                        "policy_mode": _first_nonempty(data.get("policyMode"), default="unknown"),
                        "policy_source": _first_nonempty(data.get("policySource"), default="unknown"),
                    },
                    {
                        "count": 1,
                        "queue_depth": int(data.get("queueDepth") or 0),
                        "events_enqueued": int(data.get("eventsEnqueued") or 0),
                        "events_flushed": int(data.get("eventsFlushed") or 0),
                        "send_failures": int(data.get("sendFailures") or 0),
                        "policy_enabled": bool(data.get("policyEnabled")),
                    },
                    timestamp_ns,
                )
            )
            continue
        lines.append(
            _line(
                "aw_dlp_signal",
                {
                    "host": _first_nonempty(data.get("hostname"), default=host),
                    "event_id": event_id,
                    "signal_type": signal_type,
                    "username": username,
                    "source": _first_nonempty(data.get("source"), default="unknown"),
                },
                {
                    "count": 1,
                    "document_name": _first_nonempty(data.get("documentName"), data.get("documentNameOriginal")),
                    "printer_name": _first_nonempty(data.get("printerName")),
                    "owner": _first_nonempty(data.get("owner")),
                    "session_id": int(data.get("sessionId") or 0),
                },
                timestamp_ns,
            )
        )
    return [line for line in lines if line]


def build_incident_lines(host: str, events: list[dict]) -> list[str]:
    lines: list[str] = []
    for item in events:
        normalized = normalize_incident(item, host)
        timestamp_ns = _timestamp_ns(pts(item.get("timestamp")))
        event_id = item.get("id") or f"incident-{timestamp_ns}"
        lines.append(
            _line(
                "aw_dlp_incident",
                {
                    "host": normalized["host"],
                    "event_id": event_id,
                    "signal_type": normalized["signal_type"],
                    "severity": normalized["severity"],
                    "action": normalized["action"],
                    "username": normalized["username"],
                    "source": normalized["source"],
                },
                {
                    "count": 1,
                    "message": normalized["message"],
                    "rule_id": normalized["rule_id"],
                    "document_name": normalized["document_name"],
                    "printer_name": normalized["printer_name"],
                    "incident_status": normalized["incident_status"],
                    "incident_verdict": normalized["incident_verdict"],
                    "regex_matches": int(normalized["regex_matches"]),
                    "dictionary_matches": int(normalized["dictionary_matches"]),
                    "ocr_requested": bool(normalized["ocr_requested"]),
                },
                timestamp_ns,
            )
        )
    return [line for line in lines if line]


def build_review_lines(host: str, events: list[dict]) -> list[str]:
    lines: list[str] = []
    for item in events:
        data = item.get("data") or {}
        review = data.get("review") or {}
        source_data = (data.get("sourceEvent") or {}).get("data") or {}
        timestamp_ns = _timestamp_ns(pts(item.get("timestamp")))
        review_id = _first_nonempty(review.get("reviewId"), default=f"review-{timestamp_ns}")
        lines.append(
            _line(
                "aw_dlp_review",
                {
                    "host": _first_nonempty(data.get("host"), source_data.get("hostname"), default=host),
                    "review_id": review_id,
                    "verdict": _first_nonempty(review.get("verdict"), default="unknown"),
                    "signal_type": _first_nonempty(source_data.get("signalType"), default="unknown"),
                    "username": _first_nonempty(source_data.get("username"), source_data.get("owner"), default="unknown"),
                },
                {
                    "count": 1,
                    "archived": bool(review.get("archived")),
                    "comment": _first_nonempty(review.get("comment")),
                    "category": _first_nonempty(review.get("category")),
                    "document_name": _first_nonempty(source_data.get("documentName"), source_data.get("documentNameOriginal")),
                    "printer_name": _first_nonempty(source_data.get("printerName")),
                },
                timestamp_ns,
            )
        )
    return [line for line in lines if line]


def build_rule_lines(host: str, events: list[dict]) -> list[str]:
    lines: list[str] = []
    for item in events:
        data = item.get("data") or {}
        match = data.get("match") or {}
        timestamp_ns = _timestamp_ns(pts(item.get("timestamp")))
        rule_id = _first_nonempty(data.get("ruleId"), default=f"rule-{timestamp_ns}")
        lines.append(
            _line(
                "aw_dlp_rule",
                {
                    "host": _first_nonempty(data.get("host"), match.get("hostname"), default=host),
                    "rule_id": rule_id,
                    "action": _first_nonempty(data.get("action"), default="unknown"),
                    "signal_type": _first_nonempty(match.get("signalType"), default="unknown"),
                    "username": _first_nonempty(match.get("username"), match.get("owner"), default="unknown"),
                    "enabled": "true" if bool(data.get("enabled", True)) else "false",
                },
                {
                    "count": 1,
                    "category": _first_nonempty(data.get("category")),
                    "comment": _first_nonempty(data.get("comment")),
                    "document_name": _first_nonempty(match.get("documentName")),
                    "printer_name": _first_nonempty(match.get("printerName")),
                },
                timestamp_ns,
            )
        )
    return [line for line in lines if line]


def build_fileops_lines(host: str, events: list[dict]) -> list[str]:
    lines: list[str] = []
    for item in events:
        data = item.get("data") or {}
        signal_type = _first_nonempty(data.get("signalType"), default="unknown")
        if signal_type != "collector_health":
            continue
        timestamp_ns = _timestamp_ns(pts(item.get("timestamp")))
        event_id = item.get("id") or f"fileops-{timestamp_ns}"
        lines.append(
            _line(
                "aw_dlp_fileops_health",
                {
                    "host": _first_nonempty(data.get("hostname"), default=host),
                    "event_id": event_id,
                    "username": _first_nonempty(data.get("username"), default="unknown"),
                },
                {
                    "count": 1,
                    "queue_depth": int(data.get("queueDepth") or 0),
                    "events_enqueued": int(data.get("eventsEnqueued") or 0),
                    "events_flushed": int(data.get("eventsFlushed") or 0),
                    "send_failures": int(data.get("sendFailures") or 0),
                    "session_id": int(data.get("sessionId") or 0),
                },
                timestamp_ns,
            )
        )
    return [line for line in lines if line]


def build_case_lines(host: str, cases: list[dict]) -> list[str]:
    lines: list[str] = []
    for item in cases:
        timestamp_ns = _timestamp_ns(pts(item.get("updated_at") or item.get("created_at")))
        evidence = item.get("evidence") or {}
        evidence_items = evidence.get("items") or []
        lines.append(
            _line(
                "aw_dlp_case",
                {
                    "host": _first_nonempty(item.get("host"), default=host),
                    "case_id": item.get("id"),
                    "status": _first_nonempty(item.get("status"), default="unknown"),
                    "severity": _first_nonempty(item.get("severity"), default="unknown"),
                    "assignee": _first_nonempty(item.get("assignee"), default="unassigned"),
                },
                {
                    "count": 1,
                    "title": _first_nonempty(item.get("title")),
                    "incident_id": _first_nonempty(item.get("incident_id")),
                    "has_forensics": item.get("forensics") is not None,
                    "evidence_items": len(evidence_items),
                    "chain_length": int(evidence.get("chain_length") or 0),
                },
                timestamp_ns,
            )
        )
    return [line for line in lines if line]


def build_lines_for_host(host: str, start: datetime, end: datetime) -> list[str]:
    lines: list[str] = []
    lines.extend(build_endpoint_lines(host, fetch_bucket_events(f"aw-dlp-endpoint-signals_{host}", start, end, EVENT_LIMIT)))
    lines.extend(build_incident_lines(host, fetch_bucket_events(f"aw-dlp-incidents_{host}", start, end, EVENT_LIMIT)))
    lines.extend(build_review_lines(host, fetch_bucket_events(f"aw-dlp-review_{host}", start, end, EVENT_LIMIT)))
    lines.extend(build_rule_lines(host, fetch_bucket_events(f"aw-dlp-rules_{host}", start, end, EVENT_LIMIT)))
    lines.extend(build_fileops_lines(host, fetch_bucket_events(f"aw-file-operations_{host}", start, end, EVENT_LIMIT)))
    lines.extend(build_case_lines(host, fetch_cases(host)))
    return [line for line in lines if line]


def write_lines(lines: list[str]) -> int:
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
        if response.status not in {200, 204}:
            raise RuntimeError(f"InfluxDB write failed with status={response.status}")
    return len(lines)


def main() -> int:
    if not INFLUX_ENABLED:
        print("[aw-dlp-influx-exporter] disabled by AW_DLP_INFLUX_ENABLED", file=sys.stderr)
        return 0
    end = utc_now()
    start = end - timedelta(days=LOOKBACK_DAYS)
    lines: list[str] = []
    for host in HOSTS:
        lines.extend(build_lines_for_host(host, start, end))
    written = write_lines(lines)
    print(f"[aw-dlp-influx-exporter] wrote {written} points to {INFLUX_BUCKET}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
