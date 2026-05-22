#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.modules.setdefault("clickhouse_connect", SimpleNamespace(get_client=lambda **_: None))

import load_1c_exports as loader


class Load1CExportsTests(unittest.TestCase):
    def test_business_events_mapping(self) -> None:
        row = {
            "event_time": "2026-05-22T12:00:00Z",
            "event_id": "evt-1",
            "infobase": "ИБ-1",
            "company_entity_key": "baseid:abc",
            "organization": "ООО Тест",
            "department": "Продажи",
            "doc_id": "doc-1",
            "doc_number": "0001",
            "doc_type": "Реализация",
            "registrar": "DOC-1",
            "operation_type": "sale",
            "event_kind": "posting",
            "author": "user1",
            "counterparty": "Контрагент",
            "counterparty_inn": "7700000000",
            "account_dt": "62.01",
            "account_ct": "90.01",
            "amount": "125000.50",
            "line_no": "2",
            "evidence_ref": "reglog:1",
        }
        mapped = loader.map_core_row("business_events", "events.jsonl", row)
        self.assertEqual(mapped[1], "evt-1")
        self.assertEqual(mapped[3], "baseid:abc")
        self.assertEqual(mapped[7], "0001")
        self.assertEqual(mapped[15], "62.01")
        self.assertEqual(mapped[17], 125000.5)
        self.assertEqual(mapped[19], 2)
        self.assertEqual(mapped[21], "events.jsonl")

    def test_document_changes_mapping(self) -> None:
        row = {
            "change_time": "2026-05-22T12:00:00Z",
            "change_id": "chg-1",
            "infobase": "ИБ-1",
            "company_entity_key": "basepath:c:/1c/base",
            "organization": "ООО Тест",
            "document_id": "doc-2",
            "document_number": "0002",
            "document_type": "Поступление",
            "change_kind": "requisites_change",
            "field_name": "Контрагент",
            "user": "user2",
            "before_value": "Старый",
            "after_value": "Новый",
            "risk_tag": "counterparty_change",
            "evidence_ref": "audit:2",
        }
        mapped = loader.map_core_row("document_changes", "changes.jsonl", row)
        self.assertEqual(mapped[1], "chg-1")
        self.assertEqual(mapped[3], "basepath:c:/1c/base")
        self.assertEqual(mapped[8], "requisites_change")
        self.assertEqual(mapped[12], "Новый")
        self.assertEqual(mapped[15], "changes.jsonl")

    def test_new_dataset_columns_shape(self) -> None:
        self.assertEqual(loader.RAW_TABLES["business_events"], "raw_1c_business_events")
        self.assertEqual(loader.CORE_TABLES["document_changes"], "document_change_events")
        self.assertEqual(len(loader.core_columns("business_events")), 22)
        self.assertEqual(len(loader.core_columns("document_changes")), 16)


if __name__ == "__main__":
    unittest.main()
