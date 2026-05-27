#!/usr/bin/env python3
from __future__ import annotations

import sys
import tempfile
import unittest
from datetime import UTC, datetime
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.modules.setdefault("clickhouse_connect", SimpleNamespace(get_client=lambda **_: None))

import extract_1c_mcp_toolkit as extractor
from load_1c_exports import Config as LoaderConfig


class Extract1CMcpToolkitTests(unittest.TestCase):
    def test_validate_runtime_config_flags_invalid_enabled_query_and_limit(self) -> None:
        loader_conf = LoaderConfig(
            clickhouse={},
            landing={
                "documents": "/tmp/documents",
                "postings": "/tmp/postings",
                "business_events": "/tmp/business_events",
                "document_changes": "/tmp/document_changes",
                "companies": "/tmp/companies",
                "reglog": "/tmp/reglog",
                "audit": "/tmp/audit",
                "host": "/tmp/host",
            },
            formats={"default": "jsonl"},
            archive_dir=None,
            delete_after_load=False,
            min_file_age_seconds=180,
        )
        toolkit_conf = extractor.ToolkitConfig(
            base_url="http://127.0.0.1:6003",
            channel="prod",
            timeout_seconds=30,
            verify_tls=True,
            state_dir="./state",
            datasets={
                "documents": extractor.QueryDatasetConfig(enabled=True, query="", limit=1001),
                "postings": extractor.QueryDatasetConfig(enabled=False, query="SELECT"),
                "business_events": extractor.QueryDatasetConfig(enabled=False, query="SELECT"),
                "document_changes": extractor.QueryDatasetConfig(enabled=False, query="SELECT"),
                "companies": extractor.QueryDatasetConfig(enabled=False, query="SELECT"),
            },
            event_log=extractor.EventLogConfig(enabled=False, limit=500),
        )
        issues = extractor.validate_runtime_config(loader_conf, toolkit_conf)
        self.assertIn("mcp_toolkit.datasets.documents.query is required when dataset is enabled", issues)
        self.assertIn("mcp_toolkit.datasets.documents.limit should not exceed 1000 for 1c-mcp-toolkit execute_query", issues)

    def test_normalize_query_row_derives_entity_key_and_evidence(self) -> None:
        spec = extractor.QueryDatasetConfig(
            enabled=True,
            query="SELECT",
            field_map={"Номер": "document_number", "База": "infobase", "Путь": "base_path"},
            static_fields={"organization": "ООО Тест"},
        )
        row = {
            "Номер": "0001",
            "База": "ФЕЛИЦТ ГРУПП 2026",
            "Путь": r"c:\1c\felict",
        }
        normalized = extractor.normalize_query_row("document_changes", row, spec)
        self.assertEqual(normalized["document_number"], "0001")
        self.assertEqual(normalized["organization"], "ООО Тест")
        self.assertTrue(normalized["company_entity_key"].startswith("basepath:"))
        self.assertEqual(normalized["evidence_ref"], "mcp:document_change:0001")

    def test_resolve_incremental_bounds_applies_lookback(self) -> None:
        spec = extractor.QueryDatasetConfig(
            enabled=True,
            query="SELECT",
            incremental=extractor.IncrementalConfig(
                since_param="since_ts",
                until_param="until_ts",
                initial_since="2026-05-01T00:00:00Z",
                lookback_seconds=120,
            ),
        )
        now = datetime(2026, 5, 23, 10, 30, tzinfo=UTC)
        params, completion_cursor = extractor.resolve_incremental_bounds(
            spec,
            {"last_success_ts": "2026-05-23T10:00:00Z"},
            now,
        )
        self.assertEqual(params["since_ts"], "2026-05-23T09:58:00Z")
        self.assertEqual(params["until_ts"], "2026-05-23T10:30:00Z")
        self.assertEqual(completion_cursor, "2026-05-23T10:30:00Z")

    def test_normalize_reglog_row_renders_message(self) -> None:
        spec = extractor.EventLogConfig(
            enabled=True,
            static_fields={"infobase": "ИБ-1"},
        )
        row = {
            "date": "2026-05-23T11:00:00",
            "user": "Иванов И.И.",
            "computer": "WS-01",
            "application": "ThinClient",
            "event": "_$Data$_.Post",
            "level": "Error",
            "comment": "Ошибка проведения",
            "metadata": "Документ.РеализацияТоваровУслуг",
            "transaction_status": "RolledBack",
            "session": 42,
        }
        normalized = extractor.normalize_reglog_row(row, spec)
        self.assertEqual(normalized["infobase"], "ИБ-1")
        self.assertEqual(normalized["host"], "WS-01")
        self.assertEqual(normalized["app"], "ThinClient")
        self.assertEqual(normalized["event_name"], "_$Data$_.Post")
        self.assertIn("metadata=Документ.РеализацияТоваровУслуг", normalized["message"])
        self.assertIn("txn=RolledBack", normalized["message"])

    def test_extract_reglog_dataset_updates_cursor_and_writes_jsonl(self) -> None:
        class FakeClient:
            def __init__(self) -> None:
                self.calls: list[dict[str, object]] = []
                self.responses = [
                    {
                        "success": True,
                        "data": [
                            {
                                "date": "2026-05-23T11:00:00",
                                "user": "USER1",
                                "computer": "WS-01",
                                "application": "ThinClient",
                                "event": "_$Data$_.Post",
                                "level": "Error",
                                "comment": "Ошибка 1",
                            }
                        ],
                        "last_date": "2026-05-23T11:00:00",
                        "next_same_second_offset": 1,
                        "has_more": True,
                    },
                    {
                        "success": True,
                        "data": [
                            {
                                "date": "2026-05-23T11:00:01",
                                "user": "USER2",
                                "computer": "WS-02",
                                "application": "ThinClient",
                                "event": "_$Data$_.Update",
                                "level": "Warning",
                                "comment": "Изменение",
                            }
                        ],
                        "last_date": "2026-05-23T11:00:01",
                        "next_same_second_offset": 0,
                        "has_more": False,
                    },
                ]

            def post_json(self, endpoint: str, payload: dict[str, object]) -> dict[str, object]:
                self.calls.append({"endpoint": endpoint, "payload": payload})
                return self.responses.pop(0)

        now = datetime(2026, 5, 23, 12, 0, tzinfo=UTC)
        with tempfile.TemporaryDirectory() as tmpdir:
            loader_conf = LoaderConfig(
                clickhouse={},
                landing={
                    "documents": f"{tmpdir}/documents",
                    "postings": f"{tmpdir}/postings",
                    "business_events": f"{tmpdir}/business_events",
                    "document_changes": f"{tmpdir}/document_changes",
                    "companies": f"{tmpdir}/companies",
                    "reglog": f"{tmpdir}/reglog",
                    "audit": f"{tmpdir}/audit",
                    "host": f"{tmpdir}/host",
                },
                formats={"default": "jsonl"},
                archive_dir=None,
                delete_after_load=False,
                min_file_age_seconds=180,
            )
            toolkit_conf = extractor.ToolkitConfig(
                base_url="http://127.0.0.1:6003",
                channel="prod",
                timeout_seconds=30,
                verify_tls=True,
                state_dir=f"{tmpdir}/state",
                datasets={dataset: extractor.QueryDatasetConfig(enabled=False, query="") for dataset in extractor.QUERY_DATASETS},
                event_log=extractor.EventLogConfig(enabled=True, initial_start_date="2026-05-23T10:00:00Z", static_fields={"infobase": "ИБ-1"}),
            )
            state = {"datasets": {}, "reglog": {}}
            client = FakeClient()
            options = extractor.RunOptions(dry_run=False, sample_size=3, max_pages=1, validate_config=False)

            rows = extractor.extract_reglog_dataset(client, loader_conf, toolkit_conf, state, now, options)

            self.assertEqual(rows, 2)
            self.assertEqual(client.calls[0]["endpoint"], "get_event_log")
            self.assertEqual(client.calls[1]["payload"]["start_date"], "2026-05-23T11:00:00")
            self.assertEqual(client.calls[1]["payload"]["same_second_offset"], 1)
            self.assertEqual(state["reglog"]["last_date"], "2026-05-23T11:00:01")
            self.assertEqual(state["reglog"]["same_second_offset"], 0)
            reglog_files = list(Path(loader_conf.landing["reglog"]).glob("*.jsonl"))
            self.assertEqual(len(reglog_files), 1)
            payload = reglog_files[0].read_text(encoding="utf-8")
            self.assertIn('"infobase": "ИБ-1"', payload)
            self.assertIn('"event_name": "_$Data$_.Update"', payload)

    def test_extract_query_dataset_dry_run_does_not_write_or_update_state(self) -> None:
        class FakeClient:
            def __init__(self) -> None:
                self.calls: list[dict[str, object]] = []

            def post_json(self, endpoint: str, payload: dict[str, object]) -> dict[str, object]:
                self.calls.append({"endpoint": endpoint, "payload": payload})
                return {
                    "success": True,
                    "data": [
                        {
                            "infobase": "ИБ-1",
                            "doc_number": "0001",
                            "doc_id": "DOC-1",
                            "amount": 10,
                        }
                    ],
                    "schema": {"columns": [{"name": "doc_number", "types": ["Строка"]}]},
                }

        now = datetime(2026, 5, 23, 12, 0, tzinfo=UTC)
        with tempfile.TemporaryDirectory() as tmpdir:
            loader_conf = LoaderConfig(
                clickhouse={},
                landing={
                    "documents": f"{tmpdir}/documents",
                    "postings": f"{tmpdir}/postings",
                    "business_events": f"{tmpdir}/business_events",
                    "document_changes": f"{tmpdir}/document_changes",
                    "companies": f"{tmpdir}/companies",
                    "reglog": f"{tmpdir}/reglog",
                    "audit": f"{tmpdir}/audit",
                    "host": f"{tmpdir}/host",
                },
                formats={"default": "jsonl"},
                archive_dir=None,
                delete_after_load=False,
                min_file_age_seconds=180,
            )
            spec = extractor.QueryDatasetConfig(
                enabled=True,
                query="SELECT *",
                limit=500,
                incremental=extractor.IncrementalConfig(since_param="since_ts", until_param="until_ts", initial_since="2026-05-01T00:00:00Z"),
            )
            state = {"datasets": {"documents": {"last_success_ts": "2026-05-23T10:00:00Z"}}, "reglog": {}}
            client = FakeClient()
            options = extractor.RunOptions(dry_run=True, sample_size=2, max_pages=1, validate_config=False)

            rows = extractor.extract_query_dataset("documents", spec, client, loader_conf, state, now, options)

            self.assertEqual(rows, 1)
            self.assertEqual(client.calls[0]["endpoint"], "execute_query")
            self.assertEqual(client.calls[0]["payload"]["limit"], 2)
            self.assertTrue(client.calls[0]["payload"]["include_schema"])
            self.assertEqual(state["datasets"]["documents"]["last_success_ts"], "2026-05-23T10:00:00Z")
            self.assertFalse(Path(loader_conf.landing["documents"]).exists())


if __name__ == "__main__":
    unittest.main()
