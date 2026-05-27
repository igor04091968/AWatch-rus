#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import ssl
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

import yaml

from build_business_event_exports import canonical_company_entity_key, write_jsonl
from load_1c_exports import Config as LoaderConfig
from load_1c_exports import load_config, normalize_ts

QUERY_DATASETS = ("documents", "postings", "business_events", "document_changes", "companies")
SUPPORTED_DATASETS = QUERY_DATASETS + ("reglog",)


@dataclass(frozen=True)
class IncrementalConfig:
    since_param: str = ""
    until_param: str = ""
    initial_since: str = ""
    lookback_seconds: int = 0


@dataclass(frozen=True)
class QueryDatasetConfig:
    enabled: bool
    query: str
    params: dict[str, Any] = field(default_factory=dict)
    limit: int = 1000
    include_schema: bool = False
    field_map: dict[str, str] = field(default_factory=dict)
    static_fields: dict[str, Any] = field(default_factory=dict)
    incremental: IncrementalConfig | None = None


@dataclass(frozen=True)
class EventLogConfig:
    enabled: bool
    initial_start_date: str = ""
    lookback_seconds: int = 0
    limit: int = 500
    levels: list[str] = field(default_factory=list)
    events: list[str] = field(default_factory=list)
    field_map: dict[str, str] = field(default_factory=dict)
    static_fields: dict[str, Any] = field(default_factory=dict)
    filters: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ToolkitConfig:
    base_url: str
    channel: str
    timeout_seconds: float
    verify_tls: bool
    state_dir: str
    datasets: dict[str, QueryDatasetConfig]
    event_log: EventLogConfig


@dataclass(frozen=True)
class RunOptions:
    dry_run: bool
    sample_size: int
    max_pages: int
    validate_config: bool


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Extract read-only 1C data from 1c-mcp-toolkit into clickhouse-1c landing dirs")
    parser.add_argument("--config", required=True, help="Path to YAML config")
    parser.add_argument("--dataset", choices=SUPPORTED_DATASETS, help="Extract only one dataset")
    parser.add_argument("--dry-run", action="store_true", help="Probe endpoint/query pack safely without writing landing files or updating state")
    parser.add_argument("--sample-size", type=int, default=3, help="Rows to preview in dry-run mode")
    parser.add_argument("--max-pages", type=int, default=1, help="Max pages to read in dry-run reglog probing")
    parser.add_argument("--validate-config", action="store_true", help="Validate config statically and exit")
    return parser.parse_args()


def load_runtime_config(path: str) -> tuple[LoaderConfig, ToolkitConfig]:
    loader_conf = load_config(path)
    raw = yaml.safe_load(Path(path).read_text(encoding="utf-8"))
    toolkit_raw = raw.get("mcp_toolkit")
    if not isinstance(toolkit_raw, dict):
        raise ValueError("missing mcp_toolkit config block")

    datasets: dict[str, QueryDatasetConfig] = {}
    for dataset in QUERY_DATASETS:
        dataset_raw = dict(toolkit_raw.get("datasets", {}).get(dataset, {}) or {})
        incremental_raw = dict(dataset_raw.get("incremental", {}) or {})
        datasets[dataset] = QueryDatasetConfig(
            enabled=bool(dataset_raw.get("enabled", False)),
            query=str(dataset_raw.get("query", "") or "").strip(),
            params=dict(dataset_raw.get("params", {}) or {}),
            limit=int(dataset_raw.get("limit", 1000) or 1000),
            include_schema=bool(dataset_raw.get("include_schema", False)),
            field_map={str(k): str(v) for k, v in dict(dataset_raw.get("field_map", {}) or {}).items()},
            static_fields=dict(dataset_raw.get("static_fields", {}) or {}),
            incremental=IncrementalConfig(
                since_param=str(incremental_raw.get("since_param", "") or "").strip(),
                until_param=str(incremental_raw.get("until_param", "") or "").strip(),
                initial_since=str(incremental_raw.get("initial_since", "") or "").strip(),
                lookback_seconds=int(incremental_raw.get("lookback_seconds", 0) or 0),
            )
            if incremental_raw
            else None,
        )

    event_log_raw = dict(toolkit_raw.get("event_log", {}) or {})
    event_log = EventLogConfig(
        enabled=bool(event_log_raw.get("enabled", False)),
        initial_start_date=str(event_log_raw.get("initial_start_date", "") or "").strip(),
        lookback_seconds=int(event_log_raw.get("lookback_seconds", 0) or 0),
        limit=int(event_log_raw.get("limit", 500) or 500),
        levels=[str(v) for v in list(event_log_raw.get("levels", []) or []) if str(v).strip()],
        events=[str(v) for v in list(event_log_raw.get("events", []) or []) if str(v).strip()],
        field_map={str(k): str(v) for k, v in dict(event_log_raw.get("field_map", {}) or {}).items()},
        static_fields=dict(event_log_raw.get("static_fields", {}) or {}),
        filters={str(k): v for k, v in dict(event_log_raw.get("filters", {}) or {}).items()},
    )

    toolkit_conf = ToolkitConfig(
        base_url=str(toolkit_raw.get("base_url", "") or "").rstrip("/"),
        channel=str(toolkit_raw.get("channel", "") or "").strip(),
        timeout_seconds=float(toolkit_raw.get("timeout_seconds", 120) or 120),
        verify_tls=bool(toolkit_raw.get("verify_tls", True)),
        state_dir=str(toolkit_raw.get("state_dir", "./state/1c-mcp-toolkit") or "./state/1c-mcp-toolkit"),
        datasets=datasets,
        event_log=event_log,
    )
    if not toolkit_conf.base_url:
        raise ValueError("mcp_toolkit.base_url is required")
    return loader_conf, toolkit_conf


class ToolkitClient:
    def __init__(self, conf: ToolkitConfig):
        self.conf = conf
        self.ssl_context = None if conf.verify_tls else ssl._create_unverified_context()

    def post_json(self, endpoint: str, payload: dict[str, Any]) -> dict[str, Any]:
        query = urllib.parse.urlencode({"channel": self.conf.channel}) if self.conf.channel else ""
        url = f"{self.conf.base_url}/api/{endpoint}"
        if query:
            url = f"{url}?{query}"
        request = urllib.request.Request(
            url,
            data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=self.conf.timeout_seconds, context=self.ssl_context) as response:
                body = response.read().decode("utf-8")
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"{endpoint} http_error={exc.code} {detail}") from exc
        except urllib.error.URLError as exc:
            raise RuntimeError(f"{endpoint} transport_error={exc}") from exc

        payload_obj = json.loads(body)
        if not payload_obj.get("success", False):
            raise RuntimeError(f"{endpoint} api_error={payload_obj.get('error', 'unknown error')}")
        return payload_obj


def state_path(toolkit_conf: ToolkitConfig) -> Path:
    return Path(toolkit_conf.state_dir) / "extract_state.json"


def load_state(toolkit_conf: ToolkitConfig) -> dict[str, Any]:
    path = state_path(toolkit_conf)
    if not path.exists():
        return {"datasets": {}, "reglog": {}}
    return json.loads(path.read_text(encoding="utf-8"))


def save_state(toolkit_conf: ToolkitConfig, state: dict[str, Any]) -> None:
    path = state_path(toolkit_conf)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(state, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(path)


def utc_now() -> datetime:
    return datetime.now(UTC)


def isoformat_seconds(value: datetime) -> str:
    return value.astimezone(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def normalize_utc_datetime(value: Any) -> datetime:
    dt = normalize_ts(value)
    if dt.tzinfo is None:
        return dt.replace(tzinfo=UTC)
    return dt.astimezone(UTC)


def validate_runtime_config(loader_conf: LoaderConfig, toolkit_conf: ToolkitConfig) -> list[str]:
    issues: list[str] = []
    if not toolkit_conf.base_url:
        issues.append("mcp_toolkit.base_url is required")
    if not toolkit_conf.state_dir:
        issues.append("mcp_toolkit.state_dir is required")

    for dataset in QUERY_DATASETS:
        spec = toolkit_conf.datasets[dataset]
        if spec.enabled and spec.limit < 1:
            issues.append(f"mcp_toolkit.datasets.{dataset}.limit must be >= 1")
        if spec.enabled and spec.limit > 1000:
            issues.append(f"mcp_toolkit.datasets.{dataset}.limit should not exceed 1000 for 1c-mcp-toolkit execute_query")
        if spec.enabled and not spec.query:
            issues.append(f"mcp_toolkit.datasets.{dataset}.query is required when dataset is enabled")
        if spec.incremental and not spec.incremental.since_param and not spec.incremental.until_param:
            issues.append(f"mcp_toolkit.datasets.{dataset}.incremental must define at least one of since_param/until_param")
        if dataset not in loader_conf.landing:
            issues.append(f"landing.{dataset} is missing from ETL config")

    if toolkit_conf.event_log.limit < 1:
        issues.append("mcp_toolkit.event_log.limit must be >= 1")
    if toolkit_conf.event_log.enabled and "reglog" not in loader_conf.landing:
        issues.append("landing.reglog is missing from ETL config")
    return issues


def flatten_value(value: Any) -> Any:
    if value is None:
        return ""
    if isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, list):
        return json.dumps([flatten_value(item) for item in value], ensure_ascii=False)
    if isinstance(value, dict):
        if value.get("_objectRef"):
            return (
                value.get("Представление")
                or value.get("Description")
                or value.get("УникальныйИдентификатор")
                or json.dumps(value, ensure_ascii=False)
            )
        return json.dumps({key: flatten_value(item) for key, item in value.items()}, ensure_ascii=False)
    return str(value)


def apply_field_map(row: dict[str, Any], field_map: dict[str, str]) -> dict[str, Any]:
    mapped: dict[str, Any] = {}
    for key, value in row.items():
        mapped[field_map.get(str(key), str(key))] = flatten_value(value)
    return mapped


def derive_entity_key(row: dict[str, Any]) -> str:
    explicit = str(row.get("company_entity_key", "") or "").strip()
    if explicit:
        return explicit
    return canonical_company_entity_key(
        base_id=str(row.get("base_id", "") or "").strip(),
        base_path=str(row.get("base_path", "") or "").strip(),
        infobase=str(row.get("infobase", "") or "").strip(),
    )


def default_evidence_ref(dataset: str, row: dict[str, Any]) -> str:
    if dataset == "business_events":
        return f"mcp:business_event:{row.get('document_id') or row.get('document_number') or row.get('event_id') or 'unknown'}"
    if dataset == "document_changes":
        return f"mcp:document_change:{row.get('document_id') or row.get('document_number') or row.get('change_id') or 'unknown'}"
    return ""


def normalize_query_row(dataset: str, row: dict[str, Any], spec: QueryDatasetConfig) -> dict[str, Any]:
    mapped = apply_field_map(row, spec.field_map)
    normalized = {**mapped, **spec.static_fields}
    if dataset in {"documents", "postings", "business_events", "document_changes", "companies"}:
        entity_key = derive_entity_key(normalized)
        if entity_key:
            normalized["company_entity_key"] = entity_key
    if dataset == "companies" and not normalized.get("company_name"):
        normalized["company_name"] = normalized.get("counterparty") or normalized.get("organization") or normalized.get("infobase", "")
    if dataset in {"business_events", "document_changes"} and not normalized.get("evidence_ref"):
        normalized["evidence_ref"] = default_evidence_ref(dataset, normalized)
    return normalized


def render_reglog_message(row: dict[str, Any]) -> str:
    message = str(row.get("comment", "") or "").strip()
    if not message:
        message = str(row.get("data_presentation", "") or "").strip()
    extras: list[str] = []
    metadata = str(row.get("metadata", "") or "").strip()
    if metadata:
        extras.append(f"metadata={metadata}")
    transaction_status = str(row.get("transaction_status", "") or "").strip()
    if transaction_status:
        extras.append(f"txn={transaction_status}")
    session = row.get("session")
    if session not in ("", None):
        extras.append(f"session={session}")
    if extras:
        suffix = "; ".join(extras)
        return f"{message} | {suffix}" if message else suffix
    return message


def normalize_reglog_row(row: dict[str, Any], spec: EventLogConfig) -> dict[str, Any]:
    mapped = apply_field_map(row, spec.field_map)
    normalized = {
        "ts": mapped.get("ts") or mapped.get("date", ""),
        "infobase": mapped.get("infobase", ""),
        "user": mapped.get("user", ""),
        "host": mapped.get("host") or mapped.get("computer", ""),
        "app": mapped.get("app") or mapped.get("application", ""),
        "event_name": mapped.get("event_name") or mapped.get("event", ""),
        "level": mapped.get("level", "info"),
        "duration_ms": mapped.get("duration_ms", 0),
        "message": mapped.get("message") or render_reglog_message(mapped),
    }
    normalized.update(spec.static_fields)
    return normalized


def resolve_incremental_bounds(spec: QueryDatasetConfig, dataset_state: dict[str, Any], now: datetime) -> tuple[dict[str, Any], str]:
    if spec.incremental is None:
        return {}, ""

    cursor = str(dataset_state.get("last_success_ts", "") or "").strip()
    if cursor:
        since_dt = normalize_utc_datetime(cursor)
        if spec.incremental.lookback_seconds > 0:
            since_dt -= timedelta(seconds=spec.incremental.lookback_seconds)
    elif spec.incremental.initial_since:
        since_dt = normalize_utc_datetime(spec.incremental.initial_since)
    elif spec.incremental.lookback_seconds > 0:
        since_dt = now - timedelta(seconds=spec.incremental.lookback_seconds)
    else:
        since_dt = now

    params: dict[str, Any] = {}
    if spec.incremental.since_param:
        params[spec.incremental.since_param] = isoformat_seconds(since_dt)
    if spec.incremental.until_param:
        params[spec.incremental.until_param] = isoformat_seconds(now)
    return params, isoformat_seconds(now)


def build_query_payload(spec: QueryDatasetConfig, dataset_state: dict[str, Any], now: datetime) -> tuple[dict[str, Any], str]:
    payload = {
        "query": spec.query,
        "params": dict(spec.params),
        "limit": spec.limit,
        "include_schema": spec.include_schema,
    }
    window_params, completion_cursor = resolve_incremental_bounds(spec, dataset_state, now)
    payload["params"].update(window_params)
    return payload, completion_cursor


def build_query_payload_for_dry_run(spec: QueryDatasetConfig, dataset_state: dict[str, Any], now: datetime, sample_size: int) -> tuple[dict[str, Any], str]:
    payload, completion_cursor = build_query_payload(spec, dataset_state, now)
    payload["limit"] = min(max(1, sample_size), int(payload.get("limit", sample_size) or sample_size))
    payload["include_schema"] = True
    return payload, completion_cursor


def build_output_path(loader_conf: LoaderConfig, dataset: str, now: datetime) -> Path:
    landing_root = Path(loader_conf.landing[dataset])
    stamp = now.astimezone(UTC).strftime("%Y%m%dT%H%M%SZ")
    return landing_root / f"{stamp}-mcp-{dataset}.jsonl"


def compact_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True)


def print_dry_run_summary(dataset: str, payload: dict[str, Any], rows: list[dict[str, Any]], response: dict[str, Any], *, sample_size: int) -> None:
    schema = response.get("schema", {})
    schema_columns = list(schema.get("columns", []) or []) if isinstance(schema, dict) else []
    print(f"dry-run {dataset}: rows={len(rows)} schema_columns={len(schema_columns)}")
    print(f"dry-run {dataset}: request={compact_json(payload)}")
    if schema_columns:
        preview_columns = schema_columns[: min(sample_size, len(schema_columns))]
        print(f"dry-run {dataset}: schema_preview={compact_json(preview_columns)}")
    if rows:
        preview_rows = rows[: min(sample_size, len(rows))]
        print(f"dry-run {dataset}: sample={compact_json(preview_rows)}")


def extract_query_dataset(
    dataset: str,
    spec: QueryDatasetConfig,
    client: ToolkitClient,
    loader_conf: LoaderConfig,
    state: dict[str, Any],
    now: datetime,
    options: RunOptions,
) -> int:
    if not spec.enabled:
        print(f"skip {dataset}: disabled")
        return 0
    if not spec.query:
        raise ValueError(f"{dataset}: enabled but query is empty")

    dataset_state = dict(state.setdefault("datasets", {}).get(dataset, {}) or {})
    if options.dry_run:
        payload, completion_cursor = build_query_payload_for_dry_run(spec, dataset_state, now, options.sample_size)
    else:
        payload, completion_cursor = build_query_payload(spec, dataset_state, now)
    response = client.post_json("execute_query", payload)
    rows = [normalize_query_row(dataset, row, spec) for row in list(response.get("data", []) or [])]
    if options.dry_run:
        print_dry_run_summary(dataset, payload, rows, response, sample_size=options.sample_size)
        return len(rows)
    if rows:
        out = build_output_path(loader_conf, dataset, now)
        write_jsonl(out, rows, min_age_seconds=loader_conf.min_file_age_seconds)
        print(f"extracted {dataset}: rows={len(rows)} file={out}")
    else:
        print(f"extracted {dataset}: rows=0")

    if completion_cursor:
        dataset_state["last_success_ts"] = completion_cursor
    dataset_state["last_row_count"] = len(rows)
    dataset_state["last_run_at"] = isoformat_seconds(now)
    state["datasets"][dataset] = dataset_state
    return len(rows)


def build_event_log_payload(spec: EventLogConfig, cursor: dict[str, Any], end_date: str) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "end_date": end_date,
        "limit": spec.limit,
    }
    if cursor.get("start_date"):
        payload["start_date"] = cursor["start_date"]
    if cursor.get("same_second_offset", 0):
        payload["same_second_offset"] = int(cursor["same_second_offset"])
    if spec.levels:
        payload["levels"] = spec.levels
    if spec.events:
        payload["events"] = spec.events
    for key, value in spec.filters.items():
        if value not in ("", None, [], {}):
            payload[key] = value
    return payload


def initial_reglog_cursor(spec: EventLogConfig, state: dict[str, Any], now: datetime) -> dict[str, Any]:
    reglog_state = dict(state.get("reglog", {}) or {})
    if reglog_state.get("last_date"):
        return {
            "start_date": str(reglog_state.get("last_date")),
            "same_second_offset": int(reglog_state.get("same_second_offset", 0) or 0),
        }
    if spec.initial_start_date:
        start_dt = normalize_utc_datetime(spec.initial_start_date)
    elif spec.lookback_seconds > 0:
        start_dt = now - timedelta(seconds=spec.lookback_seconds)
    else:
        start_dt = now
    return {"start_date": isoformat_seconds(start_dt), "same_second_offset": 0}


def extract_reglog_dataset(
    client: ToolkitClient,
    loader_conf: LoaderConfig,
    toolkit_conf: ToolkitConfig,
    state: dict[str, Any],
    now: datetime,
    options: RunOptions,
) -> int:
    spec = toolkit_conf.event_log
    if not spec.enabled:
        print("skip reglog: disabled")
        return 0

    cursor = initial_reglog_cursor(spec, state, now)
    end_date = isoformat_seconds(now)
    rows: list[dict[str, Any]] = []
    last_date = cursor["start_date"]
    next_offset = int(cursor.get("same_second_offset", 0) or 0)
    page_no = 0

    while True:
        page_no += 1
        payload = build_event_log_payload(spec, cursor, end_date)
        if options.dry_run:
            payload["limit"] = min(max(1, options.sample_size), int(payload.get("limit", options.sample_size) or options.sample_size))
        response = client.post_json("get_event_log", payload)
        batch = [normalize_reglog_row(row, spec) for row in list(response.get("data", []) or [])]
        rows.extend(batch)
        last_date = str(response.get("last_date") or last_date or end_date)
        next_offset = int(response.get("next_same_second_offset", 0) or 0)
        has_more = bool(response.get("has_more", False))
        if options.dry_run:
            print_dry_run_summary("reglog", payload, batch, response, sample_size=options.sample_size)
            if len(rows) >= options.sample_size:
                break
            if options.max_pages > 0 and page_no >= options.max_pages:
                break
        if not has_more:
            break
        cursor = {"start_date": last_date, "same_second_offset": next_offset}

    if options.dry_run:
        print(f"dry-run reglog: total_sampled_rows={len(rows)}")
        return len(rows)

    if rows:
        out = build_output_path(loader_conf, "reglog", now)
        write_jsonl(out, rows, min_age_seconds=loader_conf.min_file_age_seconds)
        print(f"extracted reglog: rows={len(rows)} file={out}")
    else:
        print("extracted reglog: rows=0")

    state["reglog"] = {
        "last_date": last_date if rows else end_date,
        "same_second_offset": next_offset if rows else 0,
        "last_row_count": len(rows),
        "last_run_at": end_date,
    }
    return len(rows)


def main() -> int:
    args = parse_args()
    loader_conf, toolkit_conf = load_runtime_config(args.config)
    options = RunOptions(
        dry_run=bool(args.dry_run),
        sample_size=max(1, int(args.sample_size or 1)),
        max_pages=max(1, int(args.max_pages or 1)),
        validate_config=bool(args.validate_config),
    )
    issues = validate_runtime_config(loader_conf, toolkit_conf)
    if issues:
        for issue in issues:
            print(f"config-error: {issue}")
        return 2
    if options.validate_config:
        print("config-ok")
        return 0
    client = ToolkitClient(toolkit_conf)
    state = load_state(toolkit_conf) if not options.dry_run else {"datasets": {}, "reglog": {}}
    now = utc_now()

    datasets = [args.dataset] if args.dataset else list(SUPPORTED_DATASETS)
    total_rows = 0

    for dataset in datasets:
        if dataset == "reglog":
            total_rows += extract_reglog_dataset(client, loader_conf, toolkit_conf, state, now, options)
            continue
        total_rows += extract_query_dataset(dataset, toolkit_conf.datasets[dataset], client, loader_conf, state, now, options)

    if not options.dry_run:
        state["last_run_at"] = isoformat_seconds(now)
        save_state(toolkit_conf, state)
    print(f"{'dry-run' if options.dry_run else 'done'}: datasets={','.join(datasets)} total_rows={total_rows}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
