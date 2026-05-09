#!/usr/bin/env python3
"""
Extract IOC-like indicators from Sigma YAML rules for DLP preload.

Extracted fields:
  - Image|endswith
  - CommandLine|contains
  - OriginalFileName
  - Hashes|SHA256 (plus SHA256 values embedded in Hashes strings)
"""

from __future__ import annotations

import argparse
import csv
import json
import re
from pathlib import Path
from typing import Any

import yaml


BASE_FIELDS = {"image", "commandline", "originalfilename", "hashes"}
SHA256_RE = re.compile(r"\b[a-fA-F0-9]{64}\b")


def split_key(key: str) -> tuple[str, list[str]]:
    parts = [p.strip() for p in str(key).split("|") if p.strip()]
    if not parts:
        return "", []
    return parts[0].lower(), [p.lower() for p in parts[1:]]


def flatten_values(value: Any) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        v = value.strip()
        return [v] if v else []
    if isinstance(value, (int, float, bool)):
        return [str(value)]
    if isinstance(value, list):
        out: list[str] = []
        for item in value:
            out.extend(flatten_values(item))
        return out
    if isinstance(value, dict):
        out: list[str] = []
        for k, v in value.items():
            vals = flatten_values(v)
            for vv in vals:
                out.append(f"{k}:{vv}")
        return out
    return []


def detect_ioc_type(base: str, ops: list[str], raw: str) -> str | None:
    if base == "image" and "endswith" in ops:
        return "process_image_endswith"
    if base == "commandline" and "contains" in ops:
        return "commandline_contains"
    if base == "originalfilename":
        return "original_filename"
    if base == "hashes" and ("sha256" in ops or SHA256_RE.search(raw)):
        return "sha256"
    return None


def parse_sha256(raw: str) -> list[str]:
    vals = SHA256_RE.findall(raw)
    seen = set()
    out = []
    for v in vals:
        lv = v.lower()
        if lv in seen:
            continue
        seen.add(lv)
        out.append(lv)
    return out


def walk(node: Any, *, rule_id: str, rule_title: str, source_file: str, out: list[dict[str, str]]) -> None:
    if isinstance(node, dict):
        for k, v in node.items():
            base, ops = split_key(str(k))
            if base in BASE_FIELDS:
                for raw in flatten_values(v):
                    ioc_type = detect_ioc_type(base, ops, raw)
                    if not ioc_type:
                        continue
                    if ioc_type == "sha256":
                        for h in parse_sha256(raw):
                            out.append(
                                {
                                    "ioc_type": "sha256",
                                    "ioc_value": h,
                                    "field": str(k),
                                    "rule_id": rule_id,
                                    "rule_title": rule_title,
                                    "source_file": source_file,
                                }
                            )
                    else:
                        out.append(
                            {
                                "ioc_type": ioc_type,
                                "ioc_value": raw,
                                "field": str(k),
                                "rule_id": rule_id,
                                "rule_title": rule_title,
                                "source_file": source_file,
                            }
                        )
            walk(v, rule_id=rule_id, rule_title=rule_title, source_file=source_file, out=out)
    elif isinstance(node, list):
        for item in node:
            walk(item, rule_id=rule_id, rule_title=rule_title, source_file=source_file, out=out)


def extract_from_yaml(path: Path) -> list[dict[str, str]]:
    try:
        doc = yaml.safe_load(path.read_text(encoding="utf-8", errors="ignore"))
    except Exception:
        return []
    if not isinstance(doc, dict):
        return []
    detection = doc.get("detection")
    if detection is None:
        return []
    rid = str(doc.get("id") or "")
    title = str(doc.get("title") or "")
    rows: list[dict[str, str]] = []
    walk(detection, rule_id=rid, rule_title=title, source_file=str(path), out=rows)
    return rows


def dedupe(rows: list[dict[str, str]]) -> list[dict[str, str]]:
    seen = set()
    out = []
    for r in rows:
        key = (r["ioc_type"], r["ioc_value"].lower(), r["field"])
        if key in seen:
            continue
        seen.add(key)
        out.append(r)
    return out


def write_json(path: Path, rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")


def write_csv(path: Path, rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fields = ["ioc_type", "ioc_value", "field", "rule_id", "rule_title", "source_file"]
    with path.open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        for row in rows:
            w.writerow(row)


def sql_escape(s: str) -> str:
    return s.replace("'", "''")


def write_sql(path: Path, rows: list[dict[str, str]], table_name: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        f.write(
            f"CREATE TABLE IF NOT EXISTS {table_name} (\n"
            "  id INTEGER PRIMARY KEY AUTOINCREMENT,\n"
            "  ioc_type TEXT NOT NULL,\n"
            "  ioc_value TEXT NOT NULL,\n"
            "  field TEXT,\n"
            "  rule_id TEXT,\n"
            "  rule_title TEXT,\n"
            "  source_file TEXT\n"
            ");\n\n"
        )
        for r in rows:
            f.write(
                f"INSERT INTO {table_name} (ioc_type, ioc_value, field, rule_id, rule_title, source_file) VALUES "
                f"('{sql_escape(r['ioc_type'])}',"
                f"'{sql_escape(r['ioc_value'])}',"
                f"'{sql_escape(r['field'])}',"
                f"'{sql_escape(r['rule_id'])}',"
                f"'{sql_escape(r['rule_title'])}',"
                f"'{sql_escape(r['source_file'])}');\n"
            )


def main() -> int:
    ap = argparse.ArgumentParser(description="Extract IOC-like Sigma values for DLP preload.")
    ap.add_argument("--rules-root", default="rules", help="Path to hayabusa-rules root")
    ap.add_argument("--out-dir", default="ioc_export", help="Output directory")
    ap.add_argument("--table-name", default="dlp_blacklist_ioc", help="SQL table name")
    args = ap.parse_args()

    rules_root = Path(args.rules_root)
    if not rules_root.exists():
        raise SystemExit(f"rules root not found: {rules_root}")

    yaml_files = [p for p in rules_root.rglob("*") if p.is_file() and p.suffix.lower() in {".yml", ".yaml"}]
    all_rows: list[dict[str, str]] = []
    for yp in yaml_files:
        all_rows.extend(extract_from_yaml(yp))

    rows = dedupe(all_rows)
    rows.sort(key=lambda r: (r["ioc_type"], r["ioc_value"].lower()))

    out_dir = Path(args.out_dir)
    write_json(out_dir / "ioc_blacklist.json", rows)
    write_csv(out_dir / "ioc_blacklist.csv", rows)
    write_sql(out_dir / "ioc_blacklist.sql", rows, args.table_name)

    counts: dict[str, int] = {}
    for r in rows:
        counts[r["ioc_type"]] = counts.get(r["ioc_type"], 0) + 1

    print(f"rules_scanned={len(yaml_files)}")
    print(f"iocs_extracted={len(rows)}")
    for k in sorted(counts):
        print(f"{k}={counts[k]}")
    print(f"json={out_dir / 'ioc_blacklist.json'}")
    print(f"csv={out_dir / 'ioc_blacklist.csv'}")
    print(f"sql={out_dir / 'ioc_blacklist.sql'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

