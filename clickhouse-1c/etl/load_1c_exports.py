#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import shutil
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import yaml
from dateutil import parser as date_parser

RAW_TABLES = {
    "documents": "raw_1c_documents",
    "postings": "raw_1c_postings",
    "business_events": "raw_1c_business_events",
    "document_changes": "raw_1c_document_changes",
    "companies": "raw_1c_companies",
    "reglog": "raw_reglog",
    "audit": "raw_audit",
    "host": "raw_host_metrics",
}

CORE_TABLES = {
    "documents": "documents",
    "postings": "postings",
    "business_events": "business_events",
    "document_changes": "document_change_events",
    "companies": "companies",
    "reglog": "reglog_events",
    "audit": "audit_events",
    "host": "host_events",
}


@dataclass
class Config:
    clickhouse: dict[str, Any]
    landing: dict[str, str]
    formats: dict[str, str]
    archive_dir: str | None
    delete_after_load: bool
    min_file_age_seconds: int


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Load file-based 1C exports into ClickHouse")
    p.add_argument("--config", required=True, help="Path to YAML config")
    p.add_argument("--dataset", choices=["documents", "postings", "business_events", "document_changes", "companies", "reglog", "audit", "host"], help="Load only one dataset")
    return p.parse_args()


def load_config(path: str) -> Config:
    raw = yaml.safe_load(Path(path).read_text(encoding="utf-8"))
    return Config(
        clickhouse=raw["clickhouse"],
        landing=raw["landing"],
        formats=raw.get("formats", {"default": "jsonl"}),
        archive_dir=raw.get("archive_dir"),
        delete_after_load=bool(raw.get("delete_after_load", False)),
        min_file_age_seconds=int(raw.get("min_file_age_seconds", 180)),
    )


def ch_client(conf: Config):
    import clickhouse_connect

    return clickhouse_connect.get_client(
        host=conf.clickhouse["host"],
        port=conf.clickhouse.get("port", 8123),
        username=conf.clickhouse.get("username", "default"),
        password=conf.clickhouse.get("password", ""),
        database=conf.clickhouse.get("database", "analytics_1c"),
    )


def normalize_ts(value: Any) -> datetime:
    if isinstance(value, datetime):
        return value
    if value is None or value == "":
        return datetime.utcnow()
    return date_parser.parse(str(value))


def iter_rows(path: Path, fmt: str) -> list[dict[str, Any]]:
    if fmt == "jsonl":
        return [json.loads(line) for line in path.read_text(encoding="utf-8-sig").splitlines() if line.strip()]
    if fmt == "json":
        payload = json.loads(path.read_text(encoding="utf-8-sig"))
        return payload if isinstance(payload, list) else [payload]
    if fmt == "csv":
        with path.open("r", encoding="utf-8-sig", newline="") as fh:
            return list(csv.DictReader(fh))
    raise ValueError(f"unsupported format: {fmt}")


def insert_raw(client, dataset: str, source_file: str, rows: list[dict[str, Any]]) -> None:
    client.insert(
        RAW_TABLES[dataset],
        [[source_file, json.dumps(row, ensure_ascii=False)] for row in rows],
        column_names=["source_file", "payload"],
    )


def map_core_row(dataset: str, source_file: str, row: dict[str, Any]) -> list[Any]:
    if dataset == "documents":
        return [
            normalize_ts(row.get("ts") or row.get("posted_at") or row.get("created_at")),
            row.get("infobase", ""),
            row.get("organization", ""),
            row.get("department", ""),
            row.get("doc_type", ""),
            row.get("doc_id", ""),
            row.get("doc_number", ""),
            row.get("author", ""),
            row.get("counterparty", ""),
            row.get("operation_type", ""),
            float(row.get("amount", 0) or 0),
            row.get("status", ""),
            int(row.get("posted", 0) or 0),
            source_file,
        ]
    if dataset == "postings":
        return [
            normalize_ts(row.get("ts")),
            row.get("infobase", ""),
            row.get("registrar", ""),
            row.get("operation_type", ""),
            row.get("account_dt", ""),
            row.get("account_ct", ""),
            float(row.get("amount", 0) or 0),
            source_file,
        ]
    if dataset == "business_events":
        return [
            normalize_ts(row.get("ts") or row.get("event_time")),
            row.get("event_id", ""),
            row.get("infobase", ""),
            row.get("company_entity_key", ""),
            row.get("organization", ""),
            row.get("department", ""),
            row.get("document_id", row.get("doc_id", "")),
            row.get("document_number", row.get("doc_number", "")),
            row.get("document_type", row.get("doc_type", "")),
            row.get("registrar", ""),
            row.get("operation_type", ""),
            row.get("event_kind", ""),
            row.get("user", row.get("author", "")),
            row.get("counterparty", ""),
            row.get("counterparty_inn", ""),
            row.get("debit_account", row.get("account_dt", "")),
            row.get("credit_account", row.get("account_ct", "")),
            float(row.get("amount", 0) or 0),
            row.get("currency", "RUB"),
            int(row.get("line_no", 0) or 0),
            row.get("evidence_ref", ""),
            source_file,
        ]
    if dataset == "document_changes":
        return [
            normalize_ts(row.get("ts") or row.get("change_time")),
            row.get("change_id", ""),
            row.get("infobase", ""),
            row.get("company_entity_key", ""),
            row.get("organization", ""),
            row.get("document_id", row.get("doc_id", "")),
            row.get("document_number", row.get("doc_number", "")),
            row.get("document_type", row.get("doc_type", "")),
            row.get("change_kind", ""),
            row.get("field_name", ""),
            row.get("user", row.get("author", "")),
            row.get("before_value", ""),
            row.get("after_value", ""),
            row.get("risk_tag", ""),
            row.get("evidence_ref", ""),
            source_file,
        ]
    if dataset == "companies":
        return [
            normalize_ts(row.get("ts")),
            row.get("infobase", ""),
            row.get("company_name", row.get("counterparty", row.get("infobase", ""))),
            row.get("organization", ""),
            row.get("owner_user", row.get("author", "")),
            row.get("base_id", row.get("doc_id", "")),
            row.get("base_path", ""),
            row.get("status", ""),
            int(row.get("db_size_bytes", 0) or 0),
            int(row.get("reglog_size_bytes", 0) or 0),
            int(row.get("active_locks", 0) or 0),
            int(row.get("temp_db_present", 0) or 0),
            int(row.get("scheduler_touched", 0) or 0),
            float(row.get("activity_score", row.get("amount", 0)) or 0),
            source_file,
        ]
    if dataset == "reglog":
        return [
            normalize_ts(row.get("ts")),
            row.get("infobase", ""),
            row.get("user", ""),
            row.get("host", ""),
            row.get("app", ""),
            row.get("event_name", ""),
            row.get("level", "info"),
            int(row.get("duration_ms", 0) or 0),
            row.get("message", ""),
            source_file,
        ]
    if dataset == "audit":
        return [
            normalize_ts(row.get("ts")),
            row.get("infobase", ""),
            row.get("user", ""),
            row.get("object_type", ""),
            row.get("object_id", ""),
            row.get("action", ""),
            row.get("before_hash", ""),
            row.get("after_hash", ""),
            row.get("risk_tag", ""),
            source_file,
        ]
    if dataset == "host":
        return [
            normalize_ts(row.get("ts")),
            row.get("host", ""),
            float(row.get("cpu_pct", 0) or 0),
            float(row.get("ram_pct", 0) or 0),
            float(row.get("disk_free_gb", 0) or 0),
            float(row.get("disk_latency_ms", 0) or 0),
            int(row.get("smb_errors", 0) or 0),
            int(row.get("rdp_sessions", 0) or 0),
            int(row.get("backup_ok", 0) or 0),
            source_file,
        ]
    raise ValueError(dataset)


def core_columns(dataset: str) -> list[str]:
    if dataset == "documents":
        return ["ts", "infobase", "organization", "department", "doc_type", "doc_id", "doc_number", "author", "counterparty", "operation_type", "amount", "status", "posted", "source_file"]
    if dataset == "postings":
        return ["ts", "infobase", "registrar", "operation_type", "account_dt", "account_ct", "amount", "source_file"]
    if dataset == "business_events":
        return ["ts", "event_id", "infobase", "company_entity_key", "organization", "department", "document_id", "document_number", "document_type", "registrar", "operation_type", "event_kind", "user", "counterparty", "counterparty_inn", "debit_account", "credit_account", "amount", "currency", "line_no", "evidence_ref", "source_file"]
    if dataset == "document_changes":
        return ["ts", "change_id", "infobase", "company_entity_key", "organization", "document_id", "document_number", "document_type", "change_kind", "field_name", "user", "before_value", "after_value", "risk_tag", "evidence_ref", "source_file"]
    if dataset == "companies":
        return ["ts", "infobase", "company_name", "organization", "owner_user", "base_id", "base_path", "status", "db_size_bytes", "reglog_size_bytes", "active_locks", "temp_db_present", "scheduler_touched", "activity_score", "source_file"]
    if dataset == "reglog":
        return ["ts", "infobase", "user", "host", "app", "event_name", "level", "duration_ms", "message", "source_file"]
    if dataset == "audit":
        return ["ts", "infobase", "user", "object_type", "object_id", "action", "before_hash", "after_hash", "risk_tag", "source_file"]
    return ["ts", "host", "cpu_pct", "ram_pct", "disk_free_gb", "disk_latency_ms", "smb_errors", "rdp_sessions", "backup_ok", "source_file"]


def archive_or_delete(conf: Config, dataset: str, path: Path) -> None:
    if conf.archive_dir:
        archive_root = Path(conf.archive_dir) / dataset
        archive_root.mkdir(parents=True, exist_ok=True)
        shutil.move(str(path), archive_root / path.name)
        return
    if conf.delete_after_load:
        path.unlink(missing_ok=True)


def main() -> int:
    args = parse_args()
    conf = load_config(args.config)
    client = ch_client(conf)
    datasets = [args.dataset] if args.dataset else list(RAW_TABLES)

    for dataset in datasets:
        landing = Path(conf.landing[dataset])
        fmt = conf.formats.get(dataset, conf.formats.get("default", "jsonl"))
        if not landing.exists():
            continue
        for path in sorted(p for p in landing.iterdir() if p.is_file()):
            age_seconds = max(0, int((datetime.now(UTC) - datetime.fromtimestamp(path.stat().st_mtime, UTC)).total_seconds()))
            if age_seconds < conf.min_file_age_seconds:
                print(f"skip {dataset}: {path.name} age={age_seconds}s < min_file_age_seconds={conf.min_file_age_seconds}")
                continue
            rows = iter_rows(path, fmt)
            if not rows:
                archive_or_delete(conf, dataset, path)
                continue
            insert_raw(client, dataset, path.name, rows)
            client.insert(
                CORE_TABLES[dataset],
                [map_core_row(dataset, path.name, row) for row in rows],
                column_names=core_columns(dataset),
            )
            archive_or_delete(conf, dataset, path)
            print(f"loaded {dataset}: {path.name} rows={len(rows)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
