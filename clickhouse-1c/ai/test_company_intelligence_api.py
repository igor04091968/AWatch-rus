#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.modules.setdefault("clickhouse_connect", types.SimpleNamespace(get_client=lambda **_: None))
sys.modules.setdefault("uvicorn", types.SimpleNamespace(run=lambda *args, **kwargs: None))

if "fastapi" not in sys.modules:
    fastapi_mod = types.ModuleType("fastapi")

    class DummyFastAPI:
        def __init__(self, *args, **kwargs):
            pass

        def get(self, *args, **kwargs):
            def decorator(fn):
                return fn

            return decorator

    class DummyHTTPException(Exception):
        def __init__(self, status_code: int, detail: str):
            super().__init__(detail)
            self.status_code = status_code
            self.detail = detail

    def dummy_query(default=None, **kwargs):
        return default

    fastapi_mod.FastAPI = DummyFastAPI
    fastapi_mod.HTTPException = DummyHTTPException
    fastapi_mod.Query = dummy_query
    sys.modules["fastapi"] = fastapi_mod

    responses_mod = types.ModuleType("fastapi.responses")

    class DummyResponse:
        def __init__(self, *args, **kwargs):
            self.args = args
            self.kwargs = kwargs

    responses_mod.HTMLResponse = DummyResponse
    responses_mod.PlainTextResponse = DummyResponse
    responses_mod.Response = DummyResponse
    sys.modules["fastapi.responses"] = responses_mod

import company_intelligence_api as api


class CompanyIntelligenceApiTests(unittest.TestCase):
    def test_render_problematic_companies_html(self) -> None:
        html_page = api.render_problematic_companies_html(
            [
                {
                    "counterparty": "ФЕЛИЦТ ГРУПП 2026",
                    "infobase": "ФЕЛИЦТ ГРУПП 2026",
                    "normalized_counterparty": "ФЕЛИЦТ ГРУПП",
                    "top_severity": "critical",
                    "max_score": 95,
                    "signals_total": 12,
                    "critical_total": 5,
                    "amount_30d": 270.68,
                    "amount_forecast_30d": 8120.4,
                    "top_signal_type": "open_cases",
                    "top_summary": "Есть открытые кейсы",
                }
            ],
            days=7,
        )
        self.assertIn("Проблемные компании за 7 дней", html_page)
        self.assertIn("ФЕЛИЦТ ГРУПП 2026", html_page)
        self.assertIn("/manager/company/", html_page)

    def test_load_brief_history_records_and_detail(self) -> None:
        payload = {
            "generated_at": "2026-05-22T12:00:00+00:00",
            "render_mode": "codex",
            "model": "codex",
            "brief": {"headline": "Тестовый brief"},
        }
        with tempfile.TemporaryDirectory() as tmp:
            state_dir = Path(tmp)
            history_dir = state_dir / "history"
            history_dir.mkdir(parents=True, exist_ok=True)
            (history_dir / "20260522T120000Z.json").write_text(json.dumps(payload), encoding="utf-8")
            old = os.environ.get("AW_1C_MANAGER_BRIEF_STATE_DIR")
            os.environ["AW_1C_MANAGER_BRIEF_STATE_DIR"] = str(state_dir)
            try:
                items = api.load_brief_history_records(limit=5)
                self.assertEqual(len(items), 1)
                self.assertEqual(items[0]["headline"], "Тестовый brief")
                detail = api.load_brief_history_record("20260522T120000Z.json")
                self.assertEqual(detail["brief"]["headline"], "Тестовый brief")
            finally:
                if old is None:
                    os.environ.pop("AW_1C_MANAGER_BRIEF_STATE_DIR", None)
                else:
                    os.environ["AW_1C_MANAGER_BRIEF_STATE_DIR"] = old

    def test_render_brief_delta_html(self) -> None:
        payload = {
            "generated_at": "2026-05-22T12:00:00+00:00",
            "brief": {"headline": "Тест"},
            "context": {
                "delta": {
                    "available": True,
                    "previous_generated_at": "2026-05-22T09:00:00+00:00",
                    "current_generated_at": "2026-05-22T12:00:00+00:00",
                    "summary": {
                        "critical_total_delta": 2,
                        "busy_total_delta": 1,
                        "open_cases_total_delta": 5,
                        "detections_total_delta": 4,
                        "activity_30d_total_delta": 120.5,
                        "activity_forecast_30d_total_delta": -50.25,
                    },
                    "new_critical": ["ФЕЛИЦТ ГРУПП 2026"],
                    "resolved_critical": [],
                    "entered_watchlist": ["АВКО 2026"],
                    "left_watchlist": [],
                    "top_changes": [
                        {
                            "infobase": "ФЕЛИЦТ ГРУПП 2026",
                            "company": "ФЕЛИЦТ ГРУПП 2026",
                            "change_type": "severity_up",
                            "severity_before": "high",
                            "severity_after": "critical",
                            "score_delta": 25,
                            "open_cases_delta": 3,
                            "active_locks_delta": 2,
                            "forecast_delta": -15.0,
                            "summary": "Severity high -> critical.",
                        }
                    ],
                }
            },
        }
        html_page = api.render_brief_delta_html(payload)
        self.assertIn("Что изменилось с прошлого запуска", html_page)
        self.assertIn("Ключевые изменения", html_page)
        self.assertIn("ФЕЛИЦТ ГРУПП 2026", html_page)


if __name__ == "__main__":
    unittest.main()
