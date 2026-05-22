#!/usr/bin/env python3
from __future__ import annotations

import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.modules.setdefault("clickhouse_connect", SimpleNamespace(get_client=lambda **_: None))

import build_business_event_exports as builder


class BuildBusinessEventExportsTests(unittest.TestCase):
    def test_canonical_company_entity_key(self) -> None:
        self.assertEqual(builder.canonical_company_entity_key(base_id="ABC"), "baseid:ABC")
        self.assertTrue(builder.canonical_company_entity_key(base_path=r"c:\1c\Base 1").startswith("basepath:"))
        self.assertEqual(builder.canonical_company_entity_key(infobase="ФЕЛИЦТ ГРУПП 2026"), "infobase:ФЕЛИЦТ ГРУПП")

    def test_build_document_and_posting_events(self) -> None:
        company_index = {"ТРАНСГАЗ 2026": "baseid:tgz"}
        docs = [
            {
                "ts": "2026-05-22T10:00:00Z",
                "infobase": "ТРАНСГАЗ 2026",
                "organization": "Трансгаз",
                "department": "Продажи",
                "doc_type": "Реализация",
                "doc_id": "DOC-1",
                "doc_number": "0001",
                "author": "USER1",
                "counterparty": "ООО Альфа",
                "operation_type": "sale",
                "amount": "125000.00",
            }
        ]
        by_id, by_number = builder.build_document_index_from_rows(docs, company_index)
        doc_events = builder.build_document_events(docs, "docs.jsonl", company_index)
        posting_events = builder.build_posting_events(
            [
                {
                    "ts": "2026-05-22T10:01:00Z",
                    "infobase": "ТРАНСГАЗ 2026",
                    "registrar": "DOC-1",
                    "operation_type": "sale",
                    "account_dt": "62.01",
                    "account_ct": "90.01",
                    "amount": "125000.00",
                }
            ],
            "postings.jsonl",
            company_index,
            by_id,
            by_number,
        )
        self.assertEqual(doc_events[0]["company_entity_key"], "baseid:tgz")
        self.assertEqual(doc_events[0]["event_kind"], "document_snapshot")
        self.assertEqual(posting_events[0]["document_type"], "Реализация")
        self.assertEqual(posting_events[0]["debit_account"], "62.01")
        self.assertEqual(posting_events[0]["line_no"], 1)

    def test_build_document_changes(self) -> None:
        company_index = {"ФЕЛИЦТ ГРУПП 2026": "infobase:ФЕЛИЦТ ГРУПП"}
        docs = [
            {
                "ts": "2026-05-22T10:00:00Z",
                "infobase": "ФЕЛИЦТ ГРУПП 2026",
                "organization": "Фелицт",
                "doc_type": "Корректировка",
                "doc_id": "DOC-3",
                "doc_number": "0003",
                "author": "USER4",
                "counterparty": "ООО Бета",
            }
        ]
        by_id, by_number = builder.build_document_index_from_rows(docs, company_index)
        changes = builder.build_document_changes(
            [
                {
                    "ts": "2026-05-22T10:02:00Z",
                    "infobase": "ФЕЛИЦТ ГРУПП 2026",
                    "user": "USER4",
                    "object_type": "document",
                    "object_id": "DOC-3",
                    "action": "repost",
                    "before_hash": "abc",
                    "after_hash": "def",
                    "risk_tag": "repost",
                }
            ],
            "audit.jsonl",
            company_index,
            by_id,
            by_number,
        )
        self.assertEqual(changes[0]["document_id"], "DOC-3")
        self.assertEqual(changes[0]["document_type"], "Корректировка")
        self.assertEqual(changes[0]["change_kind"], "repost")
        self.assertEqual(changes[0]["risk_tag"], "repost")

    def test_write_jsonl_ages_generated_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "events.jsonl"
            before = time.time()
            builder.write_jsonl(path, [{"event_id": "1"}], min_age_seconds=180)
            self.assertTrue(path.exists())
            self.assertLessEqual(path.stat().st_mtime, before - 5)


if __name__ == "__main__":
    unittest.main()
