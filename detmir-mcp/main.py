#!/usr/bin/env python3
from __future__ import annotations

import json
import os
from base64 import b64encode
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from urllib import parse, request

from mcp.server.fastmcp import FastMCP


def _env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


def _env_int(name: str, default: int) -> int:
    raw = os.environ.get(name)
    if raw in (None, ""):
        return default
    return int(raw)


PROJECT_ROOT = Path(__file__).resolve().parent.parent
GRAFANA_DASHBOARDS_DIR = PROJECT_ROOT / "grafana"
AW_BASE = _env("DETMIR_AW_BASE", "http://10.10.10.13:5600").rstrip("/")
WORKTIME_BASE = _env("DETMIR_WORKTIME_BASE", "http://10.10.10.13:5610").rstrip("/")
DLP_POLICY_BASE = _env("DETMIR_DLP_POLICY_BASE", "http://10.10.10.13:5601").rstrip("/")
DLP_CASE_BASE = _env("DETMIR_DLP_CASE_BASE", "http://10.10.10.13:5602").rstrip("/")
ONEC_BASE = _env("DETMIR_ONEC_BASE", "http://10.10.10.2:8710").rstrip("/")
GRAFANA_URL = _env("DETMIR_GRAFANA_URL", "http://10.10.10.11:3000").rstrip("/")
GRAFANA_USER = os.environ.get("DETMIR_GRAFANA_USER", "")
GRAFANA_PASSWORD = os.environ.get("DETMIR_GRAFANA_PASSWORD", "")
DEFAULT_HOST = _env("DETMIR_DEFAULT_HOST", "SHARKON2025")
HTTP_TIMEOUT_SECONDS = _env_int("DETMIR_HTTP_TIMEOUT_SECONDS", 10)
MCP_TRANSPORT = _env("DETMIR_MCP_TRANSPORT", "stdio")
MCP_HOST = _env("DETMIR_MCP_HOST", "127.0.0.1")
MCP_PORT = _env_int("DETMIR_MCP_PORT", 8765)
MCP_PATH = _env("DETMIR_MCP_PATH", "/mcp")
MCP_STATELESS_HTTP = _env("DETMIR_MCP_STATELESS_HTTP", "0").lower() in {"1", "true", "yes", "on"}

mcp = FastMCP(
    "detmir",
    instructions="Read-only operational facade for DetMir ActivityWatch-Russian surfaces.",
    host=MCP_HOST,
    port=MCP_PORT,
    streamable_http_path=MCP_PATH,
    json_response=True,
    stateless_http=MCP_STATELESS_HTTP,
)


def _http_json(url: str, timeout: int | None = None, headers: dict[str, str] | None = None) -> Any:
    req = request.Request(url, headers=headers or {})
    with request.urlopen(req, timeout=timeout or HTTP_TIMEOUT_SECONDS) as resp:
        charset = resp.headers.get_content_charset() or "utf-8"
        body = resp.read().decode(charset)
        return json.loads(body)


def _grafana_headers() -> dict[str, str]:
    if not GRAFANA_USER:
        return {}
    token = b64encode(f"{GRAFANA_USER}:{GRAFANA_PASSWORD}".encode("utf-8")).decode("ascii")
    return {"Authorization": f"Basic {token}"}


def _parse_ts(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(UTC)
    except ValueError:
        return None


def _age_seconds(ts: datetime | None, now: datetime | None = None) -> int | None:
    if ts is None:
        return None
    ref = now or datetime.now(UTC)
    return max(0, int((ref - ts).total_seconds()))


def _build_aw_api_base(base: str) -> str:
    if base.endswith("/api/0"):
        return base
    return base + "/api/0"


def _safe_int(value: Any, default: int = 0) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _safe_float(value: Any, default: float = 0.0) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def _rows_sorted_by_active(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(rows, key=lambda item: (_safe_int(item.get("active_seconds")), str(item.get("user_id") or "")), reverse=True)


def _latest_bucket_timestamp(aw_api_base: str, bucket_id: str, meta: dict[str, Any]) -> datetime | None:
    metadata = meta.get("metadata") or {}
    return _parse_ts(metadata.get("end"))


def _summarize_bucket_group(
    aw_api_base: str,
    buckets: dict[str, Any],
    prefix: str,
    *,
    host: str | None = None,
    max_age_seconds: int = 900,
) -> dict[str, Any]:
    matched = sorted(
        bucket_id
        for bucket_id in buckets
        if bucket_id.startswith(prefix) and (host is None or bucket_id.endswith(f"_{host}"))
    )
    if not matched:
        return {
            "prefix": prefix,
            "host": host or "",
            "bucket_count": 0,
            "status": "missing",
            "max_age_seconds": max_age_seconds,
            "stale": [],
        }
    now = datetime.now(UTC)
    stale: list[dict[str, Any]] = []
    freshest_age: int | None = None
    oldest_age: int | None = None
    for bucket_id in matched:
        ts = _latest_bucket_timestamp(aw_api_base, bucket_id, buckets.get(bucket_id, {}))
        age = _age_seconds(ts, now=now)
        if age is None:
            stale.append({"bucket_id": bucket_id, "age_seconds": None, "reason": "no_timestamp"})
            continue
        freshest_age = age if freshest_age is None else min(freshest_age, age)
        oldest_age = age if oldest_age is None else max(oldest_age, age)
        if age > max_age_seconds:
            stale.append({"bucket_id": bucket_id, "age_seconds": age, "reason": "stale"})
    status = "ok" if not stale else "degraded"
    return {
        "prefix": prefix,
        "host": host or "",
        "bucket_count": len(matched),
        "status": status,
        "max_age_seconds": max_age_seconds,
        "freshest_age_seconds": freshest_age,
        "oldest_age_seconds": oldest_age,
        "stale": stale[:10],
    }


def _count_actions_by_kind(policy: dict[str, Any]) -> dict[str, int]:
    counts: dict[str, int] = {}

    def _push(action: Any) -> None:
        key = str(action or "unknown")
        counts[key] = counts.get(key, 0) + 1

    for group in ("rules",):
        for rule in policy.get(group, []) or []:
            _push((rule or {}).get("action"))
    endpoint = policy.get("endpoint") or {}
    if isinstance(endpoint, dict):
        for rules in endpoint.values():
            for rule in rules or []:
                _push((rule or {}).get("action"))
    return counts


@mcp.tool()
def aw_health_summary() -> dict[str, Any]:
    """Summarize core ActivityWatch health and bucket inventory."""
    aw_api_base = _build_aw_api_base(AW_BASE)
    info = _http_json(f"{aw_api_base}/info")
    buckets = _http_json(f"{aw_api_base}/buckets")
    if not isinstance(buckets, dict):
        raise RuntimeError("AW bucket index is not a dict")

    prefixes = {
        "worktime_sessions": "aw-worktime-sessions_",
        "file_operations": "aw-file-operations_",
        "endpoint_signals": "aw-dlp-endpoint-signals_",
        "incidents": "aw-dlp-incidents_",
        "web_categories": "aw-detmir-web-category_",
    }
    bucket_summary = {
        name: sum(1 for bucket_id in buckets if bucket_id.startswith(prefix))
        for name, prefix in prefixes.items()
    }

    return {
        "service": "activitywatch",
        "base_url": AW_BASE,
        "hostname": info.get("hostname"),
        "version": info.get("version"),
        "device_id": info.get("device_id"),
        "testing": bool(info.get("testing")),
        "bucket_count_total": len(buckets),
        "bucket_summary": bucket_summary,
    }


@mcp.tool()
def worktime_today(host: str = DEFAULT_HOST, day: str = "today", top_n: int = 5) -> dict[str, Any]:
    """Summarize current worktime rows for one host."""
    params = {"host": host, "day": day}
    payload = _http_json(f"{WORKTIME_BASE}/reports/worktime/today?{parse.urlencode(params)}", timeout=max(HTTP_TIMEOUT_SECONDS, 30))
    rows = payload.get("rows") or []
    rows = rows if isinstance(rows, list) else []
    ordered = _rows_sorted_by_active(rows)
    return {
        "service": "aw-worktime-api",
        "base_url": WORKTIME_BASE,
        "host": payload.get("host") or host,
        "report_date": payload.get("report_date"),
        "row_count": len(rows),
        "top_rows": [
            {
                "user_id": row.get("user_id"),
                "user": row.get("user"),
                "active_seconds": row.get("active_seconds"),
                "active_hhmm": row.get("active_hhmm"),
                "sessions_count": row.get("sessions_count"),
                "samples_count": row.get("samples_count"),
                "first_activity": row.get("first_activity"),
                "last_activity": row.get("last_activity"),
            }
            for row in ordered[: max(1, top_n)]
        ],
    }


@mcp.tool()
def worktime_management(
    host: str = DEFAULT_HOST,
    day: str = "today",
    owner: str = "",
    department: str = "",
    top_n: int = 5,
) -> dict[str, Any]:
    """Summarize management worktime report and top actions."""
    params = {"host": host, "day": day}
    if owner:
        params["owner"] = owner
    if department:
        params["department"] = department
    payload = _http_json(f"{WORKTIME_BASE}/reports/worktime/management?{parse.urlencode(params)}", timeout=max(HTTP_TIMEOUT_SECONDS, 30))
    summary = payload.get("summary") or {}
    actions = payload.get("actions") or []
    rows = payload.get("rows") or []
    return {
        "service": "aw-worktime-api",
        "base_url": WORKTIME_BASE,
        "host": payload.get("host") or host,
        "report_date": payload.get("report_date"),
        "filters": payload.get("filters") or {},
        "summary": {
            "users_count": summary.get("users_count"),
            "active_users": summary.get("active_users"),
            "inactive_users": summary.get("inactive_users"),
            "portfolio_coverage_pct": summary.get("portfolio_coverage_pct"),
            "actions_count": summary.get("actions_count"),
            "critical_actions_count": summary.get("critical_actions_count"),
            "high_actions_count": summary.get("high_actions_count"),
            "calendar_total_active_hhmm": summary.get("calendar_total_active_hhmm"),
            "workday_total_active_hhmm": summary.get("workday_total_active_hhmm"),
        },
        "top_actions": [
            {
                "action_id": action.get("action_id"),
                "priority": action.get("priority"),
                "owner": action.get("owner"),
                "user_id": action.get("user_id"),
                "reason": action.get("reason"),
            }
            for action in actions[: max(1, top_n)]
        ],
        "top_rows": [
            {
                "user_id": row.get("user_id"),
                "user": row.get("user"),
                "manager_owner": row.get("manager_owner"),
                "department": row.get("department"),
                "active_hhmm": row.get("active_hhmm"),
                "coverage_pct": row.get("coverage_pct"),
                "status": row.get("status"),
            }
            for row in _rows_sorted_by_active(rows)[: max(1, top_n)]
        ],
    }


@mcp.tool()
def dlp_mode_get() -> dict[str, Any]:
    """Get current DLP mode and compact policy metadata."""
    payload = _http_json(f"{DLP_POLICY_BASE}/api/0/dlp/policies/active")
    policy = payload.get("policy") or {}
    meta = policy.get("_tsj_meta") or {}
    return {
        "service": "aw-dlp-policy-engine",
        "base_url": DLP_POLICY_BASE,
        "active": bool(payload.get("active")),
        "policy_id": payload.get("policyId"),
        "name": payload.get("name"),
        "version": payload.get("version"),
        "checksum": payload.get("checksum"),
        "updated_at_utc": payload.get("updatedAtUtc"),
        "dlp_mode": meta.get("dlp_mode", "unknown"),
        "updated_by": meta.get("updated_by", ""),
        "ioc_enabled": bool((policy.get("ioc") or {}).get("enabled")),
        "web_rules_count": len(policy.get("rules") or []),
        "endpoint_rule_groups": sorted((policy.get("endpoint") or {}).keys()),
        "action_counts": _count_actions_by_kind(policy),
    }


@mcp.tool()
def dlp_health_summary(host: str = DEFAULT_HOST) -> dict[str, Any]:
    """Summarize DLP service health and bucket freshness over HTTP-only surfaces."""
    aw_api_base = _build_aw_api_base(AW_BASE)
    aw_info = _http_json(f"{aw_api_base}/info")
    policy_health = _http_json(f"{DLP_POLICY_BASE}/healthz")
    case_health = _http_json(f"{DLP_CASE_BASE}/health")
    case_list = _http_json(f"{DLP_CASE_BASE}/api/0/dlp/cases?limit=200")
    buckets = _http_json(f"{aw_api_base}/buckets")
    if not isinstance(buckets, dict):
        raise RuntimeError("AW bucket index is not a dict")

    open_cases = 0
    if isinstance(case_list, list):
        open_cases = sum(1 for item in case_list if str((item or {}).get("status") or "").lower() == "open")

    return {
        "host": host,
        "aw": {
            "hostname": aw_info.get("hostname"),
            "version": aw_info.get("version"),
        },
        "policy_health": policy_health,
        "case_health": case_health,
        "open_cases_count": open_cases,
        "freshness": {
            "endpoint_signals": _summarize_bucket_group(aw_api_base, buckets, "aw-dlp-endpoint-signals_", host=host, max_age_seconds=900),
            "file_operations": _summarize_bucket_group(aw_api_base, buckets, "aw-file-operations_", host=host, max_age_seconds=900),
            "incidents": _summarize_bucket_group(aw_api_base, buckets, "aw-dlp-incidents_", host=host, max_age_seconds=86400),
        },
    }


@mcp.tool()
def grafana_overview() -> dict[str, Any]:
    """Summarize Grafana health and version-controlled dashboard inventory."""
    health = _http_json(f"{GRAFANA_URL}/api/health", headers=_grafana_headers())
    dashboards = []
    if GRAFANA_DASHBOARDS_DIR.exists():
        dashboards = sorted(path.name for path in GRAFANA_DASHBOARDS_DIR.glob("*.json"))
    return {
        "service": "grafana",
        "base_url": GRAFANA_URL,
        "remote_health": health,
        "repo_dashboard_count": len(dashboards),
        "repo_dashboards": dashboards,
    }


@mcp.tool()
def onec_manager_brief() -> dict[str, Any]:
    """Summarize the latest 1C manager brief."""
    payload = _http_json(f"{ONEC_BASE}/api/1/analytics-1c/manager/brief/latest", timeout=max(HTTP_TIMEOUT_SECONDS, 30))
    context = payload.get("context") or {}
    portfolio = context.get("portfolio_summary") or {}
    freshness = context.get("freshness") or []
    stale_sources = [
        {
            "source": item.get("source"),
            "lag_hours": item.get("lag_hours"),
            "latest_ts": item.get("latest_ts"),
        }
        for item in freshness
        if bool((item or {}).get("stale"))
    ]
    brief = payload.get("brief") or {}
    top_risks = brief.get("top_risks") or context.get("top_risks") or []
    actions = brief.get("actions") or []
    return {
        "service": "company-intelligence-api",
        "base_url": ONEC_BASE,
        "generated_at": payload.get("generated_at"),
        "render_mode": payload.get("render_mode"),
        "model": payload.get("model"),
        "portfolio_summary": {
            "companies_total": portfolio.get("companies_total"),
            "critical_total": portfolio.get("critical_total"),
            "busy_total": portfolio.get("busy_total"),
            "open_cases_total": portfolio.get("open_cases_total"),
            "detections_total": portfolio.get("detections_total"),
            "activity_30d_total": portfolio.get("activity_30d_total"),
            "activity_forecast_30d_total": portfolio.get("activity_forecast_30d_total"),
        },
        "stale_sources": stale_sources[:10],
        "top_risks": [
            {
                "company": item.get("company") or item.get("counterparty") or item.get("infobase"),
                "severity": item.get("severity") or item.get("signal_severity"),
                "reason": item.get("reason") or item.get("top_signal"),
                "recommended_action": item.get("recommended_action"),
            }
            for item in top_risks[:5]
        ],
        "actions": actions[:5],
    }


def main() -> None:
    mcp.run(transport=MCP_TRANSPORT)


if __name__ == "__main__":
    main()
