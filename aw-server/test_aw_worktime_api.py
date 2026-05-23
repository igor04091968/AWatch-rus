#!/usr/bin/env python3
import importlib.util
from datetime import datetime, timezone
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("aw-worktime-api.py")
SPEC = importlib.util.spec_from_file_location("aw_worktime_api", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def _event(ts, username, session_id, active, **extra):
    data = {
        "username": username,
        "userId": f"WORKGROUP\\{username}",
        "sessionId": session_id,
        "state": "Активно" if active else "Диск",
        "active": active,
    }
    data.update(extra)
    return {"timestamp": ts, "duration": 0.0, "data": data}


def test_aggregate_rows_uses_sample_seconds_and_merges_overlap():
    start = datetime(2026, 5, 14, 6, 0, 0, tzinfo=timezone.utc)
    end = datetime(2026, 5, 14, 6, 59, 59, tzinfo=timezone.utc)
    rows = MODULE.aggregate_rows(
        [
            _event("2026-05-14T06:00:00Z", "user5", 4, True, sampleSeconds=30),
            _event("2026-05-14T06:00:30Z", "user5", 4, True, sampleSeconds=30),
            _event("2026-05-14T06:00:15Z", "user5", 5, True, sampleSeconds=30),
            _event("2026-05-14T06:01:00Z", "user5", 4, False, sampleSeconds=30),
        ],
        start,
        end,
        "SHARKON2025",
    )
    assert len(rows) == 1
    row = rows[0]
    assert row["user"] == "user5"
    assert row["user_id"] == "SHARKON2025\\user5"
    assert row["active_seconds"] == 60
    assert row["active_hhmm"] == "00:01"
    assert row["sessions_count"] == 2
    assert row["samples_count"] == 4
    assert row["active_samples"] == 3
    assert row["first_activity"] == "2026-05-14T06:00:00Z"
    assert row["last_activity"] == "2026-05-14T06:01:00Z"


def test_aggregate_rows_falls_back_to_next_sample_delta():
    start = datetime(2026, 5, 14, 7, 0, 0, tzinfo=timezone.utc)
    end = datetime(2026, 5, 14, 7, 59, 59, tzinfo=timezone.utc)
    rows = MODULE.aggregate_rows(
        [
            _event("2026-05-14T07:00:00Z", "user1", 3, True),
            _event("2026-05-14T07:00:05Z", "user1", 3, True),
            _event("2026-05-14T07:00:10Z", "user1", 3, False),
        ],
        start,
        end,
        "SHARKON2025",
    )
    row = rows[0]
    assert row["active_seconds"] == 10
    assert row["active_hhmm"] == "00:00"


def test_build_aw_api_base_accepts_root_and_api_urls():
    assert MODULE.build_aw_api_base("http://127.0.0.1:5600") == "http://127.0.0.1:5600/api/0"
    assert MODULE.build_aw_api_base("http://127.0.0.1:5600/") == "http://127.0.0.1:5600/api/0"
    assert MODULE.build_aw_api_base("http://127.0.0.1:5600/api/0") == "http://127.0.0.1:5600/api/0"


def test_aggregate_hourly_rows_splits_interval_by_local_hour():
    start = datetime(2026, 5, 14, 6, 0, 0, tzinfo=timezone.utc)
    end = datetime(2026, 5, 14, 8, 59, 59, tzinfo=timezone.utc)
    rows = MODULE.aggregate_hourly_rows(
        [
            _event("2026-05-14T06:50:00Z", "user5", 4, True, sampleSeconds=1800),
            _event("2026-05-14T07:20:00Z", "user5", 4, True, sampleSeconds=1800),
        ],
        start,
        end,
        "SHARKON2025",
    )
    assert [row["hour_local"] for row in rows] == ["09:00", "10:00"]
    assert [row["active_seconds"] for row in rows] == [600, 3000]


def test_build_management_payload_creates_actions_for_missing_and_late_users():
    rows = [
        {
            "user": "user1",
            "user_id": "SHARKON2025\\user1",
            "active_seconds": 0,
            "active_hhmm": "00:00",
            "first_activity": "",
            "last_activity": "",
            "idle_seconds": 86400,
            "sessions_count": 1,
            "samples_count": 10,
            "active_samples": 0,
            "_intervals": [],
        },
        {
            "user": "user5",
            "user_id": "SHARKON2025\\user5",
            "active_seconds": 5400,
            "active_hhmm": "01:30",
            "first_activity": "2026-05-14T08:30:00Z",
            "last_activity": "2026-05-14T10:00:00Z",
            "idle_seconds": 81000,
            "sessions_count": 1,
            "samples_count": 180,
            "active_samples": 180,
            "_intervals": [
                (
                    datetime(2026, 5, 14, 8, 30, 0, tzinfo=timezone.utc),
                    datetime(2026, 5, 14, 10, 0, 0, tzinfo=timezone.utc),
                )
            ],
        },
    ]
    payload = MODULE.build_management_payload(rows, "SHARKON2025", datetime(2026, 5, 14, tzinfo=timezone.utc).date())
    assert payload["summary"]["users_count"] == 2
    assert payload["summary"]["inactive_users"] == 1
    assert payload["summary"]["actions_count"] >= 2
    action_ids = {action["action_id"] for action in payload["actions"]}
    assert "missing_activity" in action_ids
    assert "late_start_review" in action_ids
    assert payload["actions"][0]["priority"] == "critical"


def test_build_management_payload_uses_workday_window_not_midnight_activity():
    rows = [
        {
            "user": "администратор",
            "user_id": "SHARKON2025\\Администратор",
            "active_seconds": 39600,
            "active_hhmm": "11:00",
            "first_activity": "2026-05-14T00:00:00Z",
            "last_activity": "2026-05-14T11:00:00Z",
            "idle_seconds": 0,
            "sessions_count": 1,
            "samples_count": 100,
            "active_samples": 100,
            "_intervals": [
                (
                    datetime(2026, 5, 14, 0, 0, 0, tzinfo=timezone.utc),
                    datetime(2026, 5, 14, 11, 0, 0, tzinfo=timezone.utc),
                )
            ],
        }
    ]
    payload = MODULE.build_management_payload(rows, "SHARKON2025", datetime(2026, 5, 14, tzinfo=timezone.utc).date())
    roster = payload["rows"][0]
    assert roster["calendar_active_hhmm"] == "11:00"
    assert roster["workday_active_hhmm"] == "02:00"
    assert roster["coverage_pct"] == 22.22
    assert payload["summary"]["calendar_total_active_hhmm"] == "11:00"
    assert payload["summary"]["workday_total_active_hhmm"] == "02:00"


def test_build_management_payload_applies_alias_and_executive_summary():
    original = MODULE.load_manager_aliases
    try:
        MODULE.load_manager_aliases = lambda: {
            "sharkon2025\\user1": {
                "display_name": "Иван Петров",
                "manager": "Руководитель смены",
                "department": "Бухгалтерия",
                "role": "Оператор 1С",
            }
        }
        rows = [
            {
                "user": "user1",
                "user_id": "SHARKON2025\\user1",
                "active_seconds": 0,
                "active_hhmm": "00:00",
                "first_activity": "",
                "last_activity": "",
                "idle_seconds": 86400,
                "sessions_count": 1,
                "samples_count": 10,
                "active_samples": 0,
                "_intervals": [],
            }
        ]
        payload = MODULE.build_management_payload(rows, "SHARKON2025", datetime(2026, 5, 14, tzinfo=timezone.utc).date())
        row = payload["rows"][0]
        assert row["user"] == "Иван Петров"
        assert row["manager_owner"] == "Руководитель смены"
        assert row["department"] == "Бухгалтерия"
        assert payload["actions"][0]["owner"] == "Руководитель смены"
        assert payload["executive"]["portfolio_state"] == "critical"
        assert payload["executive"]["focus_items"][0]["owner"] == "Руководитель смены"
    finally:
        MODULE.load_manager_aliases = original


def test_render_management_html_contains_action_queue():
    payload = {
        "generated_at_utc": "2026-05-14T12:00:00Z",
        "host": "SHARKON2025",
        "report_date": "2026-05-14",
        "report_timezone": "Europe/Moscow",
        "workday": {
            "start_local": "2026-05-14T09:00:00+03:00",
            "end_local": "2026-05-14T18:00:00+03:00",
            "expected_seconds_per_user": 32400,
            "expected_hhmm_per_user": "09:00",
            "target_coverage_pct": 75,
            "low_coverage_pct": 35,
        },
        "summary": {
            "users_count": 1,
            "active_users": 0,
            "inactive_users": 1,
            "on_target_users": 0,
            "below_target_users": 0,
            "portfolio_coverage_pct": 0.0,
            "actions_count": 1,
            "critical_actions_count": 1,
            "high_actions_count": 0,
            "total_active_seconds": 0,
            "total_active_hhmm": "00:00",
            "calendar_total_active_hhmm": "00:00",
            "workday_total_active_hhmm": "00:00",
            "first_activity": "",
            "last_activity": "",
            "top_user": "",
            "top_user_active_hhmm": "00:00",
        },
        "actions": [
            {
                "action_id": "missing_activity",
                "priority": "critical",
                "owner": "Руководитель смены",
                "user_id": "SHARKON2025\\user1",
                "deadline_hint": "today",
                "reason": "Нет активности",
                "recommended_action": "Проверить сотрудника",
                "evidence": {},
            }
        ],
        "rows": [
            {
                "user": "Иван Петров",
                "user_id": "SHARKON2025\\user1",
                "canonical_user_id": "SHARKON2025\\user1",
                "manager_owner": "Руководитель смены",
                "department": "Бухгалтерия",
                "active_hhmm": "00:00",
                "calendar_active_hhmm": "00:00",
                "workday_active_hhmm": "00:00",
                "coverage_pct": 0.0,
                "status": "inactive",
                "first_activity_local": "",
                "last_activity_local": "",
                "workday_first_activity_local": "",
                "workday_last_activity_local": "",
                "sessions_count": 1,
            }
        ],
        "executive": {
            "portfolio_state": "critical",
            "headline": "Есть 1 критичный вопрос, требующий решения сегодня.",
            "message": "Активны 0 из 1 сотрудников. Покрытие рабочего окна 0.0%.",
            "focus_items": [
                {
                    "priority": "critical",
                    "owner": "Руководитель смены",
                    "title": "missing_activity",
                    "reason": "Нет активности",
                    "recommended_action": "Проверить сотрудника",
                }
            ],
            "stale_sources": [],
        },
        "trend": [
            {
                "report_date": "2026-05-14",
                "users_count": 1,
                "active_users": 0,
                "inactive_users": 1,
                "workday_total_active_hhmm": "00:00",
                "portfolio_coverage_pct": 0.0,
                "actions_count": 1,
                "critical_actions_count": 1,
            }
        ],
        "sources": [
            {
                "label": "RDP worktime sessions",
                "status": "ok",
                "status_label": "fresh",
                "bucket_id": "aw-worktime-sessions_SHARKON2025",
                "timestamp": "2026-05-14T12:00:00Z",
                "age_seconds": 30,
                "required": True,
                "summary": "fresh (30s)",
                "event_summary": "queue=0 failures=0 flushed=10",
            }
        ],
        "bucket_id": "aw-worktime-sessions_SHARKON2025",
        "report_bounds": {
            "start_utc": "2026-05-13T21:00:00Z",
            "end_utc": "2026-05-14T20:59:59Z",
        },
    }
    html = MODULE.render_management_html(payload, selected_day="today")
    assert "Управленческий отчёт по работе в RDP" in html
    assert "Очередь действий руководителя" in html
    assert "missing_activity" in html
    assert "Тренд за" in html
    assert "Свежесть источников данных" in html
    assert "Что делать сегодня" in html
    assert "Иван Петров" in html
    assert "Руководитель смены" in html


def test_build_source_freshness_uses_freshest_candidate_bucket():
    original = MODULE.latest_bucket_event
    try:
        def fake_latest(bucket_id):
            if bucket_id == "aw-file-operations_SHARKON2025":
                return {
                    "timestamp": "2026-05-14T12:00:00Z",
                    "data": {"signalType": "collector_health", "queueDepth": 9, "sendFailures": 3, "eventsFlushed": 10},
                }
            if bucket_id == "aw-file-operations_10.10.10.13":
                return {
                    "timestamp": "2026-05-23T12:00:00Z",
                    "data": {"signalType": "collector_health", "queueDepth": 0, "sendFailures": 0, "eventsFlushed": 50},
                }
            return None

        MODULE.latest_bucket_event = fake_latest
        sources, actions = MODULE.build_source_freshness("SHARKON2025")
        file_source = next(source for source in sources if source["source_id"] == "file_operations")
        assert file_source["bucket_id"] == "aw-file-operations_10.10.10.13"
        assert file_source["status"] == "ok"
        assert not any(action["evidence"].get("source_id") == "file_operations" for action in actions)
    finally:
        MODULE.latest_bucket_event = original
