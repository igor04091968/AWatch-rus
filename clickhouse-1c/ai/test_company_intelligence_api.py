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
    def test_root_redirects_to_manager_brief(self) -> None:
        response = api.root()
        self.assertEqual(response.kwargs.get("status_code"), 307)
        self.assertEqual(response.kwargs.get("headers", {}).get("Location"), "/manager/brief")

    def test_api_health_alias_delegates_to_health(self) -> None:
        original = api.health
        try:
            api.health = lambda: {"status": "ok", "source": "health"}
            self.assertEqual(api.api_health(), {"status": "ok", "source": "health"})
        finally:
            api.health = original

    def test_render_manager_brief_includes_executive_regulation(self) -> None:
        payload = {
            "generated_at": "2026-05-22T12:00:00+00:00",
            "render_mode": "codex",
            "brief": {
                "headline": "Портфель 1С под давлением.",
                "summary": ["Кейсы растут.", "Нужен triage.", "Manual-match требует осторожности."],
                "manager_questions": [
                    {
                        "question": "Что происходит сейчас?",
                        "answer": "Портфель под операционным давлением.",
                        "recommended_action": "Взять 5 компаний первой очереди.",
                    }
                ],
                "management_plan": [
                    {
                        "horizon": "today",
                        "focus": "Остановить прирост проблем.",
                        "action": "Разобрать компании первой очереди.",
                        "expected_effect": "Остановить рост хвоста кейсов.",
                        "metric": "open_cases_total к вечеру.",
                    }
                ],
                "top_risks": [],
                "top_forecasts": [],
                "actions": ["Разобрать 5 компаний первой очереди."],
                "caveats": ["Operational severity не равна финансам."],
            },
            "context": {
                "portfolio_summary": {
                    "companies_total": 41,
                    "critical_total": 41,
                    "open_cases_total": 2744,
                    "activity_forecast_30d_total": 1723233.3,
                },
                "freshness": [
                    {
                        "source": "documents_ts",
                        "latest_ts": "2026-05-22T09:00:02+00:00",
                        "lag_hours": 3.84,
                        "stale": False,
                    }
                ],
            },
        }
        html_page = api.render_manager_brief_html(payload)
        self.assertIn("Регламент руководителя: утро", html_page)
        self.assertIn("Регламент руководителя: вечер", html_page)
        self.assertIn("Жёсткие правила управления", html_page)
        self.assertIn("Простые ответы для руководителя", html_page)
        self.assertIn("План действий руководителя", html_page)
        self.assertIn("Что происходит сейчас?", html_page)
        self.assertIn("Остановить прирост проблем.", html_page)

    def test_build_company_priority_context(self) -> None:
        latest_payload = {
            "generated_at": "2026-05-22T12:00:00+00:00",
            "context": {
                "delta": {
                    "top_changes": [
                        {
                            "infobase": "ФЕЛИЦТ ГРУПП 2026",
                            "company": "ФЕЛИЦТ ГРУПП 2026",
                            "priority_tier": "critical",
                            "priority_score": 190,
                            "priority_reason": "рост кейсов +5, рост блокировок +2",
                            "summary": "Открытых кейсов стало больше: 1 -> 6.",
                            "open_cases_delta": 5,
                            "active_locks_delta": 2,
                            "detections_delta": 4,
                            "forecast_delta": -22.0,
                        }
                    ]
                }
            },
            "brief": {"headline": "test"},
        }
        history_payload = {
            "generated_at": "2026-05-22T12:00:00+00:00",
            "context": {
                "portfolio_summary": {
                    "companies_total": 41,
                    "critical_total": 39,
                    "high_total": 2,
                    "busy_total": 40,
                    "open_cases_total": 100,
                    "detections_total": 100,
                    "activity_30d_total": 1000.0,
                    "activity_forecast_30d_total": 2000.0,
                },
                "delta": latest_payload["context"]["delta"],
            },
        }
        summary_payload = {
            "card": {
                "counterparty": "ФЕЛИЦТ ГРУПП 2026",
                "infobase": "ФЕЛИЦТ ГРУПП 2026",
                "signal_severity": "critical",
                "signal_score": 95,
                "open_cases_total": 6,
                "detections_total": 9,
                "active_locks": 2,
                "days_since_last_activity": 0,
                "registry_match_mode": "manual",
            }
        }
        with tempfile.TemporaryDirectory() as tmp:
            state_dir = Path(tmp)
            history_dir = state_dir / "history"
            history_dir.mkdir(parents=True, exist_ok=True)
            (state_dir / "latest.json").write_text(json.dumps(latest_payload), encoding="utf-8")
            (history_dir / "20260522T120000Z.json").write_text(json.dumps(history_payload), encoding="utf-8")
            old = os.environ.get("AW_1C_MANAGER_BRIEF_STATE_DIR")
            os.environ["AW_1C_MANAGER_BRIEF_STATE_DIR"] = str(state_dir)
            try:
                context = api.build_company_priority_context(summary_payload, "ФЕЛИЦТ ГРУПП 2026")
                self.assertEqual(context["current_priority_tier"], "critical")
                self.assertIn("рост кейсов", context["current_priority_reason"])
                self.assertTrue(any("manual" in item.lower() for item in context["evidence"]))
            finally:
                if old is None:
                    os.environ.pop("AW_1C_MANAGER_BRIEF_STATE_DIR", None)
                else:
                    os.environ["AW_1C_MANAGER_BRIEF_STATE_DIR"] = old

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

    def test_render_management_actions_html(self) -> None:
        html_page = api.render_management_actions_html(
            [
                {
                    "company_entity_key": "baseid:test",
                    "counterparty": "ФЕЛИЦТ ГРУПП 2026",
                    "infobase": "ФЕЛИЦТ ГРУПП 2026",
                    "owner_name": "Иванов",
                    "action_type": "case_triage",
                    "priority_tier": "critical",
                    "priority_score": 91,
                    "deadline_hint": "today",
                    "recommended_action": "Разобрать открытые кейсы.",
                    "reason": "Открытых кейсов 6, active detections 9.",
                    "evidence_summary": "severity=critical, cases=6",
                }
            ],
            [
                {
                    "company_entity_key": "baseid:test",
                    "counterparty": "ФЕЛИЦТ ГРУПП 2026",
                    "infobase": "ФЕЛИЦТ ГРУПП 2026",
                    "owner_names": "Иванов",
                    "priority_tier": "critical",
                    "actions_total": 1,
                    "action_types_summary": "case_triage",
                    "recommended_action_summary": "Разобрать открытые кейсы.",
                    "reason_summary": "Открытых кейсов 6, active detections 9.",
                }
            ],
            priority_tier="critical",
        )
        self.assertIn("Очередь управленческих действий по 1С", html_page)
        self.assertIn("Что делать по предприятиям", html_page)
        self.assertIn("JSON по предприятиям", html_page)
        self.assertIn("Приоритет считается по риску предприятия.", html_page)
        self.assertIn("ФЕЛИЦТ ГРУПП 2026", html_page)
        self.assertIn("Разобрать открытые кейсы.", html_page)
        self.assertIn("priority_tier=critical", html_page)

    def test_manager_company_actions_api_marks_enterprise_priority_model(self) -> None:
        original = api.management_company_actions
        try:
            api.management_company_actions = lambda priority_tier=None, owner=None, limit=100: [
                {"counterparty": "ФЕЛИЦТ ГРУПП 2026", "priority_tier": "critical"}
            ]
            payload = api.manager_company_actions_api(priority_tier="critical", owner="Иванов", limit=10)
        finally:
            api.management_company_actions = original
        self.assertEqual(payload["scope"], "companies")
        self.assertEqual(payload["priority_model"], "enterprise_risk")
        self.assertEqual(payload["owner_mode"], "secondary_operational_metadata")
        self.assertEqual(payload["count"], 1)
        self.assertEqual(payload["priority_tier"], "critical")
        self.assertEqual(payload["owner"], "Иванов")

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

    def test_build_weekly_trend_report_and_render(self) -> None:
        payloads = [
            {
                "generated_at": "2026-05-21T12:00:00+00:00",
                "context": {
                    "portfolio_summary": {
                        "companies_total": 41,
                        "critical_total": 39,
                        "high_total": 2,
                        "busy_total": 40,
                        "open_cases_total": 100,
                        "detections_total": 100,
                        "activity_30d_total": 1000.0,
                        "activity_forecast_30d_total": 2000.0,
                    },
                    "delta": {"top_changes": []},
                },
            },
            {
                "generated_at": "2026-05-22T12:00:00+00:00",
                "context": {
                    "portfolio_summary": {
                        "companies_total": 41,
                        "critical_total": 41,
                        "high_total": 0,
                        "busy_total": 41,
                        "open_cases_total": 135,
                        "detections_total": 130,
                        "activity_30d_total": 1200.0,
                        "activity_forecast_30d_total": 2500.0,
                    },
                    "delta": {
                        "top_changes": [
                            {
                                "infobase": "ФЕЛИЦТ ГРУПП 2026",
                                "company": "ФЕЛИЦТ ГРУПП 2026",
                                "change_type": "cases_up",
                                "open_cases_delta": 5,
                                "active_locks_delta": 2,
                                "forecast_delta": -20.0,
                                "priority_score": 180,
                                "priority_tier": "critical",
                                "priority_reason": "рост кейсов +5, рост блокировок +2",
                            }
                        ]
                    },
                },
            },
        ]
        report = api.build_weekly_trend_report(payloads, days=7)
        self.assertEqual(len(report["daily"]), 2)
        self.assertEqual(report["top_weekly_changes"][0]["company"], "ФЕЛИЦТ ГРУПП 2026")
        html_page = api.render_weekly_trend_html(report)
        self.assertIn("Недельный тренд портфеля", html_page)
        self.assertIn("Недельный рейтинг приоритетов", html_page)
        self.assertIn("ФЕЛИЦТ ГРУПП 2026", html_page)

    def test_render_weekly_digest_html(self) -> None:
        payload = {
            "generated_at": "2026-05-22T12:00:00+00:00",
            "context": {
                "period_start": "2026-05-16",
                "period_end": "2026-05-22",
                "latest_summary": {
                    "companies_total": 41,
                    "critical_total": 39,
                    "busy_total": 40,
                    "open_cases_total": 100,
                    "detections_total": 110,
                    "activity_forecast_30d_total": 2500.0,
                },
            },
            "digest": {
                "headline": "Неделя тяжёлая: кейсы и busy растут.",
                "summary": ["critical +2", "кейсы +15", "manual-match не равен юр. факту"],
                "top_priorities": [
                    {
                        "company": "ФЕЛИЦТ ГРУПП 2026",
                        "priority": "critical",
                        "reason": "рост кейсов +5, рост блокировок +2",
                        "recommended_action": "Сразу разобрать новые открытые кейсы.",
                    }
                ],
                "improvements": [
                    {
                        "company": "СЕРДИТОВ АНДРЕЙ 2026",
                        "signal": "severity снизилась high -> medium",
                        "meaning": "Нагрузка ослабла.",
                    }
                ],
                "actions": ["Разобрать top priority компании."],
                "caveats": ["Активность не равна выручке."],
            },
        }
        html_page = api.render_weekly_digest_html(payload)
        self.assertIn("Неделя тяжёлая", html_page)
        self.assertIn("Компании первой очереди", html_page)
        self.assertIn("Что улучшилось за неделю", html_page)

    def test_build_company_recovery_context_from_latest_recovery(self) -> None:
        recovery_payload = {
            "generated_at": "2026-05-22T13:00:00+00:00",
            "render_mode": "codex",
            "recovery": {
                "headline": "Recovery test",
                "top_incidents": [
                    {
                        "company": "ФЕЛИЦТ ГРУПП 2026",
                        "severity": "critical",
                        "diagnosis": "Открытые кейсы 6, detections 9, блокировки 2.",
                        "actions": ["Закрыть минимум 3 кейса.", "Проверить lock-контур."],
                        "stop_doing": "Не тянуть хвост без владельца.",
                        "target_state_24h": "Кейсы <= 3 и нет нового прироста.",
                    }
                ],
            },
        }
        summary_payload = {
            "card": {
                "counterparty": "ФЕЛИЦТ ГРУПП 2026",
                "infobase": "ФЕЛИЦТ ГРУПП 2026",
                "signal_severity": "critical",
                "signal_score": 95,
                "open_cases_total": 6,
                "detections_total": 9,
                "active_locks": 2,
                "registry_match_mode": "manual",
            },
            "priority_context": {
                "current_priority_tier": "critical",
                "current_priority_score": 190,
                "current_priority_reason": "рост кейсов +5",
                "actions": ["Разобрать кейсы."],
            },
        }
        with tempfile.TemporaryDirectory() as tmp:
            state_dir = Path(tmp)
            (state_dir / "latest.json").write_text(json.dumps(recovery_payload), encoding="utf-8")
            old = os.environ.get("AW_1C_RECOVERY_BRIEF_STATE_DIR")
            os.environ["AW_1C_RECOVERY_BRIEF_STATE_DIR"] = str(state_dir)
            try:
                context = api.build_company_recovery_context(summary_payload, "ФЕЛИЦТ ГРУПП 2026")
                self.assertEqual(context["confidence"], "recovery-brief/codex")
                self.assertIn("Открытые кейсы 6", context["diagnosis"])
                self.assertIn("Закрыть минимум 3 кейса.", context["actions"])
            finally:
                if old is None:
                    os.environ.pop("AW_1C_RECOVERY_BRIEF_STATE_DIR", None)
                else:
                    os.environ["AW_1C_RECOVERY_BRIEF_STATE_DIR"] = old

    def test_render_recovery_brief_html_and_company_page_block(self) -> None:
        recovery_payload = {
            "generated_at": "2026-05-22T13:00:00+00:00",
            "render_mode": "codex",
            "recovery": {
                "headline": "Recovery-контур под давлением.",
                "situation": ["Кейсы растут.", "Нужен triage."],
                "portfolio_actions": ["Сжать фокус до 5 компаний."],
                "top_incidents": [
                    {
                        "company": "ФЕЛИЦТ ГРУПП 2026",
                        "severity": "critical",
                        "diagnosis": "Открытые кейсы 6.",
                        "actions": ["Закрыть минимум 3 кейса."],
                        "stop_doing": "Не тянуть хвост без владельца.",
                        "target_state_24h": "Кейсы <= 3.",
                    }
                ],
                "caveats": ["Operational severity не равна финансам."],
            },
        }
        html_page = api.render_recovery_brief_html(recovery_payload)
        self.assertIn("Recovery-контур под давлением", html_page)
        self.assertIn("Компании первой очереди для recovery", html_page)
        self.assertIn("ФЕЛИЦТ ГРУПП 2026", html_page)

        company_html = api.render_company_detail_html(
            {
                "essence": "test",
                "card": {
                    "counterparty": "ФЕЛИЦТ ГРУПП 2026",
                    "company_name": "ФЕЛИЦТ ГРУПП 2026",
                    "normalized_counterparty": "ФЕЛИЦТ ГРУПП",
                    "infobase": "ФЕЛИЦТ ГРУПП 2026",
                    "signal_severity": "critical",
                    "signal_score": 95,
                    "amount_7d": 10.0,
                    "amount_30d": 20.0,
                    "amount_forecast_30d": 30.0,
                    "docs_30d": 6,
                    "open_cases_total": 6,
                    "detections_total": 9,
                    "current_status": "busy",
                    "active_locks": 2,
                    "days_since_last_activity": 0,
                    "registry_match_mode": "manual",
                    "registry_assignee_name": "Иванов",
                    "registry_inn": "123",
                    "registry_kpp": "456",
                    "base_path": "C:/1C",
                },
                "company_state": {
                    "current_status": "busy",
                    "active_locks": 2,
                    "current_activity_score": 45.0,
                    "ts": "2026-05-22T13:00:00+00:00",
                },
                "forecasts": [],
                "signals": [],
                "recent_documents": [],
                "management_actions": [
                    {
                        "priority_tier": "critical",
                        "recommended_action": "Разобрать открытые кейсы.",
                        "deadline_hint": "today",
                        "reason": "Открытых кейсов 6, active detections 9.",
                    }
                ],
                "priority_context": {
                    "current_priority_tier": "critical",
                    "current_priority_score": 190,
                    "current_priority_reason": "рост кейсов +5",
                    "verdict": "Приоритет высокий.",
                    "evidence": ["Кейсы растут."],
                    "actions": ["Разобрать кейсы."],
                },
                "recovery_context": {
                    "generated_at": "2026-05-22T13:00:00+00:00",
                    "confidence": "recovery-brief/codex",
                    "diagnosis": "Открытые кейсы 6.",
                    "actions": ["Закрыть минимум 3 кейса."],
                    "stop_doing": "Не тянуть хвост без владельца.",
                    "target_state_24h": "Кейсы <= 3.",
                },
            }
        )
        self.assertIn("AI-план снятия проблемы", company_html)
        self.assertIn("Закрыть минимум 3 кейса.", company_html)
        self.assertIn("Текущие управленческие действия по компании", company_html)
        self.assertIn("Разобрать открытые кейсы.", company_html)


if __name__ == "__main__":
    unittest.main()
