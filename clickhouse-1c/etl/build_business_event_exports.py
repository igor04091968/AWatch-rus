#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from load_1c_exports import Config, iter_rows, load_config, normalize_ts


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Build canonical business-event exports from read-only 1C file exports")
    p.add_argument("--config", required=True, help="Path to YAML config")
    return p.parse_args()


@dataclass(frozen=True)
class DocumentMeta:
    infobase: str
    company_entity_key: str
    organization: str
    department: str
    document_id: str
    document_number: str
    document_type: str
    registrar: str
    user: str
    counterparty: str
    operation_type: str
    amount: float


def collapse_ws(value: str) -> str:
    return re.sub(r"\s+", " ", value).strip()


def normalize_base_path(value: str) -> str:
    text = value.upper().replace("\\", "/")
    text = re.sub(r"/+", "/", text)
    text = re.sub(r"[^0-9A-ZА-ЯЁ:/._ -]+", " ", text)
    return collapse_ws(text)


def normalize_infobase_key(value: str) -> str:
    text = value.upper()
    text = re.sub(r"(^|\s)20[0-9]{2}($|\s)", " ", text)
    text = re.sub(r"[^0-9A-ZА-ЯЁ]+", " ", text)
    return collapse_ws(text)


def canonical_company_entity_key(base_id: str = "", base_path: str = "", infobase: str = "") -> str:
    if base_id:
        return f"baseid:{base_id.strip()}"
    if base_path:
        normalized_path = normalize_base_path(base_path)
        if normalized_path:
            return f"basepath:{normalized_path}"
    normalized_infobase = normalize_infobase_key(infobase)
    return f"infobase:{normalized_infobase}" if normalized_infobase else ""


def source_format(conf: Config, dataset: str) -> str:
    return conf.formats.get(dataset, conf.formats.get("default", "jsonl"))


def landing_root(conf: Config, dataset: str) -> Path:
    return Path(conf.landing[dataset])


def file_ready(path: Path, conf: Config) -> bool:
    age_seconds = max(0, int((datetime.now(UTC) - datetime.fromtimestamp(path.stat().st_mtime, UTC)).total_seconds()))
    return age_seconds >= conf.min_file_age_seconds


def iter_dataset_files(conf: Config, dataset: str) -> list[Path]:
    root = landing_root(conf, dataset)
    if not root.exists():
        return []
    return [p for p in sorted(root.iterdir()) if p.is_file() and file_ready(p, conf)]


def stable_id(prefix: str, *parts: Any) -> str:
    payload = "|".join(str(part or "") for part in parts)
    digest = hashlib.sha1(payload.encode("utf-8"), usedforsecurity=False).hexdigest()[:16]
    return f"{prefix}:{digest}"


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not rows:
        path.unlink(missing_ok=True)
        return
    tmp = path.with_suffix(path.suffix + ".tmp")
    payload = "\n".join(json.dumps(row, ensure_ascii=False) for row in rows) + "\n"
    tmp.write_text(payload, encoding="utf-8")
    tmp.replace(path)


def load_company_index(conf: Config) -> dict[str, str]:
    latest: dict[str, tuple[datetime, str]] = {}
    for path in iter_dataset_files(conf, "companies"):
        for row in iter_rows(path, source_format(conf, "companies")):
            infobase = str(row.get("infobase", "")).strip()
            if not infobase:
                continue
            entity_key = canonical_company_entity_key(
                base_id=str(row.get("base_id", "")).strip(),
                base_path=str(row.get("base_path", "")).strip(),
                infobase=infobase,
            )
            ts = normalize_ts(row.get("ts"))
            current = latest.get(infobase)
            if current is None or ts >= current[0]:
                latest[infobase] = (ts, entity_key)
    return {infobase: entity_key for infobase, (_ts, entity_key) in latest.items()}


def derive_company_entity_key(row: dict[str, Any], company_index: dict[str, str]) -> str:
    explicit = str(row.get("company_entity_key", "")).strip()
    if explicit:
        return explicit
    infobase = str(row.get("infobase", "")).strip()
    return company_index.get(infobase, "") or canonical_company_entity_key(
        base_id=str(row.get("base_id", "")).strip(),
        base_path=str(row.get("base_path", "")).strip(),
        infobase=infobase,
    )


def build_document_index(conf: Config, company_index: dict[str, str]) -> tuple[dict[tuple[str, str], DocumentMeta], dict[tuple[str, str], DocumentMeta]]:
    rows: list[dict[str, Any]] = []
    for path in iter_dataset_files(conf, "documents"):
        rows.extend(iter_rows(path, source_format(conf, "documents")))
    return build_document_index_from_rows(rows, company_index)


def build_document_index_from_rows(rows: list[dict[str, Any]], company_index: dict[str, str]) -> tuple[dict[tuple[str, str], DocumentMeta], dict[tuple[str, str], DocumentMeta]]:
    by_id: dict[tuple[str, str], DocumentMeta] = {}
    by_number: dict[tuple[str, str], DocumentMeta] = {}
    for row in rows:
        infobase = str(row.get("infobase", "")).strip()
        document_id = str(row.get("doc_id", row.get("document_id", ""))).strip()
        document_number = str(row.get("doc_number", row.get("document_number", ""))).strip()
        if not infobase:
            continue
        meta = DocumentMeta(
            infobase=infobase,
            company_entity_key=derive_company_entity_key(row, company_index),
            organization=str(row.get("organization", "")).strip(),
            department=str(row.get("department", "")).strip(),
            document_id=document_id,
            document_number=document_number,
            document_type=str(row.get("doc_type", row.get("document_type", ""))).strip(),
            registrar=document_id or document_number,
            user=str(row.get("author", row.get("user", ""))).strip(),
            counterparty=str(row.get("counterparty", "")).strip(),
            operation_type=str(row.get("operation_type", "")).strip(),
            amount=float(row.get("amount", 0) or 0),
        )
        if document_id:
            by_id[(infobase, document_id)] = meta
        if document_number:
            by_number[(infobase, document_number)] = meta
    return by_id, by_number


def lookup_document_meta(
    row: dict[str, Any],
    by_id: dict[tuple[str, str], DocumentMeta],
    by_number: dict[tuple[str, str], DocumentMeta],
) -> DocumentMeta | None:
    infobase = str(row.get("infobase", "")).strip()
    registrar = str(row.get("registrar", row.get("document_id", row.get("doc_id", "")))).strip()
    document_number = str(row.get("document_number", row.get("doc_number", ""))).strip()
    if infobase and registrar and (infobase, registrar) in by_id:
        return by_id[(infobase, registrar)]
    if infobase and document_number and (infobase, document_number) in by_number:
        return by_number[(infobase, document_number)]
    return None


def build_document_events(rows: list[dict[str, Any]], source_file: str, company_index: dict[str, str]) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    for row in rows:
        infobase = str(row.get("infobase", "")).strip()
        document_id = str(row.get("doc_id", row.get("document_id", ""))).strip()
        document_number = str(row.get("doc_number", row.get("document_number", ""))).strip()
        document_type = str(row.get("doc_type", row.get("document_type", ""))).strip()
        ts = normalize_ts(row.get("ts") or row.get("posted_at") or row.get("created_at"))
        company_entity_key = derive_company_entity_key(row, company_index)
        events.append(
            {
                "ts": ts.isoformat(),
                "event_id": stable_id("document_snapshot", source_file, infobase, document_id, document_number, ts.isoformat()),
                "infobase": infobase,
                "company_entity_key": company_entity_key,
                "organization": str(row.get("organization", "")).strip(),
                "department": str(row.get("department", "")).strip(),
                "document_id": document_id,
                "document_number": document_number,
                "document_type": document_type,
                "registrar": document_id or document_number,
                "operation_type": str(row.get("operation_type", "")).strip(),
                "event_kind": "document_snapshot",
                "user": str(row.get("author", row.get("user", ""))).strip(),
                "counterparty": str(row.get("counterparty", "")).strip(),
                "counterparty_inn": str(row.get("counterparty_inn", "")).strip(),
                "debit_account": "",
                "credit_account": "",
                "amount": float(row.get("amount", 0) or 0),
                "currency": str(row.get("currency", "RUB")).strip() or "RUB",
                "line_no": 0,
                "evidence_ref": f"document:{document_id or document_number}",
            }
        )
    return events


def build_posting_events(
    rows: list[dict[str, Any]],
    source_file: str,
    company_index: dict[str, str],
    by_id: dict[tuple[str, str], DocumentMeta],
    by_number: dict[tuple[str, str], DocumentMeta],
) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    for idx, row in enumerate(rows, start=1):
        meta = lookup_document_meta(row, by_id, by_number)
        infobase = str(row.get("infobase", "")).strip()
        registrar = str(row.get("registrar", "")).strip()
        ts = normalize_ts(row.get("ts"))
        company_entity_key = meta.company_entity_key if meta else derive_company_entity_key(row, company_index)
        line_no = int(row.get("line_no", idx) or idx)
        events.append(
            {
                "ts": ts.isoformat(),
                "event_id": stable_id("posting", source_file, infobase, registrar, line_no, ts.isoformat()),
                "infobase": infobase,
                "company_entity_key": company_entity_key,
                "organization": meta.organization if meta else str(row.get("organization", "")).strip(),
                "department": meta.department if meta else str(row.get("department", "")).strip(),
                "document_id": meta.document_id if meta else registrar,
                "document_number": meta.document_number if meta else str(row.get("document_number", "")).strip(),
                "document_type": meta.document_type if meta else str(row.get("document_type", "")).strip(),
                "registrar": registrar,
                "operation_type": str(row.get("operation_type", meta.operation_type if meta else "")).strip(),
                "event_kind": "posting",
                "user": meta.user if meta else str(row.get("user", row.get("author", ""))).strip(),
                "counterparty": meta.counterparty if meta else str(row.get("counterparty", "")).strip(),
                "counterparty_inn": str(row.get("counterparty_inn", "")).strip(),
                "debit_account": str(row.get("account_dt", row.get("debit_account", ""))).strip(),
                "credit_account": str(row.get("account_ct", row.get("credit_account", ""))).strip(),
                "amount": float(row.get("amount", 0) or 0),
                "currency": str(row.get("currency", "RUB")).strip() or "RUB",
                "line_no": line_no,
                "evidence_ref": f"posting:{registrar}:{line_no}",
            }
        )
    return events


def build_document_changes(
    rows: list[dict[str, Any]],
    source_file: str,
    company_index: dict[str, str],
    by_id: dict[tuple[str, str], DocumentMeta],
    by_number: dict[tuple[str, str], DocumentMeta],
) -> list[dict[str, Any]]:
    changes: list[dict[str, Any]] = []
    for row in rows:
        infobase = str(row.get("infobase", "")).strip()
        object_type = str(row.get("object_type", "")).strip()
        object_id = str(row.get("object_id", "")).strip()
        ts = normalize_ts(row.get("ts"))
        meta = lookup_document_meta(
            {
                "infobase": infobase,
                "registrar": row.get("document_id") or (object_id if object_type == "document" else ""),
                "document_number": row.get("document_number", ""),
            },
            by_id,
            by_number,
        )
        company_entity_key = meta.company_entity_key if meta else derive_company_entity_key(row, company_index)
        document_id = meta.document_id if meta else (object_id if object_type == "document" else str(row.get("document_id", "")).strip())
        changes.append(
            {
                "ts": ts.isoformat(),
                "change_id": stable_id("change", source_file, infobase, object_type, object_id, row.get("action", ""), ts.isoformat()),
                "infobase": infobase,
                "company_entity_key": company_entity_key,
                "organization": meta.organization if meta else str(row.get("organization", "")).strip(),
                "document_id": document_id,
                "document_number": meta.document_number if meta else str(row.get("document_number", "")).strip(),
                "document_type": meta.document_type if meta else str(row.get("document_type", "")).strip(),
                "change_kind": str(row.get("change_kind", row.get("action", object_type))).strip(),
                "field_name": str(row.get("field_name", object_type)).strip(),
                "user": str(row.get("user", row.get("author", ""))).strip(),
                "before_value": str(row.get("before_value", row.get("before_hash", ""))).strip(),
                "after_value": str(row.get("after_value", row.get("after_hash", ""))).strip(),
                "risk_tag": str(row.get("risk_tag", "")).strip(),
                "evidence_ref": f"audit:{object_type}:{object_id}",
            }
        )
    return changes


def output_path(conf: Config, dataset: str, source_path: Path, prefix: str) -> Path:
    return landing_root(conf, dataset) / f"{prefix}-{source_path.stem}.jsonl"


def main() -> int:
    args = parse_args()
    conf = load_config(args.config)
    company_index = load_company_index(conf)
    by_id, by_number = build_document_index(conf, company_index)

    for path in iter_dataset_files(conf, "documents"):
        rows = iter_rows(path, source_format(conf, "documents"))
        out = output_path(conf, "business_events", path, "business-events-documents")
        write_jsonl(out, build_document_events(rows, path.name, company_index))
        print(f"built business_events from documents: {path.name} rows={len(rows)}")

    for path in iter_dataset_files(conf, "postings"):
        rows = iter_rows(path, source_format(conf, "postings"))
        out = output_path(conf, "business_events", path, "business-events-postings")
        write_jsonl(out, build_posting_events(rows, path.name, company_index, by_id, by_number))
        print(f"built business_events from postings: {path.name} rows={len(rows)}")

    for path in iter_dataset_files(conf, "audit"):
        rows = iter_rows(path, source_format(conf, "audit"))
        out = output_path(conf, "document_changes", path, "document-changes-audit")
        write_jsonl(out, build_document_changes(rows, path.name, company_index, by_id, by_number))
        print(f"built document_changes from audit: {path.name} rows={len(rows)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
