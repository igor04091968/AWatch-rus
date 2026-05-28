#!/usr/bin/env python3
import importlib.util
import json
import tempfile
from datetime import datetime, timezone
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("aw-worktime-api.py")
SPEC = importlib.util.spec_from_file_location("aw_worktime_api", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class _FakeWriter:
    def __init__(self, exc=None):
        self.exc = exc
        self.data = b""

    def write(self, data):
        if self.exc is not None:
            raise self.exc
        self.data += data


class _FakeHandler:
    def __init__(self, exc=None):
        self.wfile = _FakeWriter(exc=exc)
        self.status = None
        self.headers = {}
        self.ended = False

    def send_response(self, status):
        self.status = status

    def send_header(self, key, value):
        self.headers[key] = value

    def end_headers(self):
        self.ended = True


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


def test_worktime_health_payload_is_lightweight_and_ok():
    payload = MODULE.worktime_health_payload()
    assert payload["ok"] is True
    assert payload["default_host"] == MODULE.DEFAULT_HOST
    assert payload["aw_api_base"] == MODULE.AW
    assert payload["report_timezone"] == str(MODULE.REPORT_TZ)
    assert payload["generated_at_utc"].endswith("Z")


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
    assert [row["active_seconds"] for row in rows] == [300, 300]


def test_build_true_active_apps_requires_foreground_not_afk_and_evidence():
    start = datetime(2026, 5, 14, 6, 0, 0, tzinfo=timezone.utc)
    end = datetime(2026, 5, 14, 7, 59, 59, tzinfo=timezone.utc)
    window_events = [
        {"timestamp": "2026-05-14T06:00:00Z", "duration": 120, "data": {"app": "1cv8.exe", "title": "ИНФОВЕСТ"}},
        {"timestamp": "2026-05-14T06:02:00Z", "duration": 180, "data": {"app": "1cv8.exe", "title": "Счета учета: Материалы"}},
        {"timestamp": "2026-05-14T07:00:00Z", "duration": 600, "data": {"app": "totalcmd.exe", "title": "Total Commander"}},
    ]
    afk_events = [
        {"timestamp": "2026-05-14T06:00:00Z", "duration": 600, "data": {"status": "not-afk"}},
        {"timestamp": "2026-05-14T07:00:00Z", "duration": 600, "data": {"status": "not-afk"}},
    ]
    evidence_events_by_bucket = {
        "aw-file-operations_SHARKON2025": [
            {
                "timestamp": "2026-05-14T07:05:00Z",
                "duration": 0,
                "data": {"signalType": "file_write", "path": "C:\\data\\report.xlsx"},
            }
        ],
        "aw-dlp-endpoint-signals_SHARKON2025": [
            {
                "timestamp": "2026-05-14T07:06:00Z",
                "duration": 0,
                "data": {"signalType": "collector_health", "eventsFlushed": 100},
            }
        ],
    }

    rows = MODULE.build_true_active_apps_from_events(window_events, afk_events, evidence_events_by_bucket, start, end)

    by_app = {row["application"]: row for row in rows}
    assert "1С" in by_app
    assert "Total Commander" in by_app
    assert by_app["1С"]["proved_work_seconds"] == 300
    assert by_app["1С"]["last_action"] == "Счета учета: Материалы"
    assert by_app["Total Commander"]["proved_work_seconds"] == 480
    assert by_app["Total Commander"]["last_action"] == "C:\\data\\report.xlsx"


def test_render_html_contains_true_active_apps_table():
    html = MODULE.render_html(
        [],
        "SHARKON2025",
        datetime(2026, 5, 14, tzinfo=timezone.utc).date(),
        selected_day="today",
        true_active_apps=[
            {
                "application": "1С",
                "proved_work_human": "34 мин",
                "proved_work_hhmm": "00:34",
                "last_action_local": "15:31",
                "last_action": "Счета учета: Материалы",
            }
        ],
    )
    assert "Доказанная работа по приложениям" in html
    assert "Приложение" in html
    assert "Доказанная работа" in html
    assert "Последнее действие" in html
    assert "Счета учета: Материалы" in html


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
    assert roster["workday_active_hhmm"] == "05:00"
    assert roster["coverage_pct"] == 55.56
    assert payload["summary"]["calendar_total_active_hhmm"] == "11:00"
    assert payload["summary"]["workday_total_active_hhmm"] == "05:00"


def test_build_management_payload_applies_alias_and_executive_summary():
    original = MODULE.load_manager_aliases
    original_owners = MODULE.load_manager_owners
    original_sources = MODULE.build_source_freshness
    original_trend = MODULE.build_management_trend
    try:
        MODULE.load_manager_aliases = lambda: {
            "sharkon2025\\user1": {
                "display_name": "Иван Петров",
                "manager": "Руководитель смены",
                "department": "Бухгалтерия",
                "role": "Оператор 1С",
            }
        }
        MODULE.load_manager_owners = lambda: {
            "руководитель смены": {
                "display_name": "Сменный руководитель",
                "title": "Руководитель смены 1С",
                "department": "Операторы 1С",
                "contact": "@shift-lead",
                "escalation_to": "Финансовый директор",
                "notes": "Дневной контур",
            }
        }
        MODULE.build_source_freshness = lambda host: ([], [])
        MODULE.build_management_trend = lambda host, report_date, owner_filter="", department_filter="", **kwargs: []
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
        assert payload["owner_rollups"][0]["name"] == "Руководитель смены"
        assert payload["owner_rollups"][0]["critical_actions_count"] >= 1
        assert payload["department_rollups"][0]["name"] == "Бухгалтерия"
        assert payload["owner_roster"][0]["display_name"] == "Сменный руководитель"
        assert payload["owner_roster"][0]["contact"] == "@shift-lead"
    finally:
        MODULE.load_manager_aliases = original
        MODULE.load_manager_owners = original_owners
        MODULE.build_source_freshness = original_sources
        MODULE.build_management_trend = original_trend


def test_build_management_payload_filters_by_owner():
    original_aliases = MODULE.load_manager_aliases
    original_owners = MODULE.load_manager_owners
    original_trend = MODULE.build_management_trend
    original_sources = MODULE.build_source_freshness
    try:
        MODULE.load_manager_aliases = lambda: {
            "sharkon2025\\user1": {
                "display_name": "Иван Петров",
                "manager": "Руководитель смены",
                "department": "Бухгалтерия",
                "role": "Оператор 1С",
            },
            "sharkon2025\\user2": {
                "display_name": "Мария Соколова",
                "manager": "Финансовый директор",
                "department": "Финансы",
                "role": "Главбух",
            },
        }
        MODULE.load_manager_owners = lambda: {
            "руководитель смены": {"display_name": "Сменный руководитель", "title": "Руководитель смены 1С"}
        }
        MODULE.build_management_trend = lambda *args, **kwargs: []
        MODULE.build_source_freshness = lambda host: ([], [])
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
                "user": "user2",
                "user_id": "SHARKON2025\\user2",
                "active_seconds": 3600,
                "active_hhmm": "01:00",
                "first_activity": "2026-05-14T07:00:00Z",
                "last_activity": "2026-05-14T08:00:00Z",
                "idle_seconds": 82800,
                "sessions_count": 1,
                "samples_count": 120,
                "active_samples": 120,
                "_intervals": [
                    (
                        datetime(2026, 5, 14, 7, 0, 0, tzinfo=timezone.utc),
                        datetime(2026, 5, 14, 8, 0, 0, tzinfo=timezone.utc),
                    )
                ],
            },
        ]
        payload = MODULE.build_management_payload(
            rows,
            "SHARKON2025",
            datetime(2026, 5, 14, tzinfo=timezone.utc).date(),
            owner_filter="Руководитель смены",
        )
        assert payload["filters"]["owner"] == "Руководитель смены"
        assert payload["filters"]["department"] == ""
        assert payload["summary"]["users_count"] == 1
        assert [row["user"] for row in payload["rows"]] == ["Иван Петров"]
        assert len(payload["owner_rollups"]) == 1
        assert payload["owner_rollups"][0]["name"] == "Руководитель смены"
        assert len(payload["department_rollups"]) == 1
        assert payload["department_rollups"][0]["name"] == "Бухгалтерия"
        assert payload["owner_roster"][0]["display_name"] == "Сменный руководитель"
        assert all(action["owner"] == "Руководитель смены" for action in payload["actions"])
    finally:
        MODULE.load_manager_aliases = original_aliases
        MODULE.load_manager_owners = original_owners
        MODULE.build_management_trend = original_trend
        MODULE.build_source_freshness = original_sources


def _minimal_management_payload(report_date, users_count=1):
    return {
        "summary": {
            "users_count": users_count,
            "active_users": users_count,
            "inactive_users": 0,
            "workday_total_active_seconds": 3600 * users_count,
            "workday_total_active_hhmm": f"0{users_count}:00",
            "portfolio_coverage_pct": 100.0,
            "actions_count": 0,
            "critical_actions_count": 0,
        },
        "rows": [],
        "actions": [],
        "sources": [],
        "filters": {"owner": "", "department": ""},
    }


def test_load_management_cache_keeps_historical_reports_after_ttl():
    original_dir = MODULE.MANAGER_CACHE_DIR
    original_ttl = MODULE.MANAGER_CACHE_TTL_SECONDS
    try:
        with tempfile.TemporaryDirectory() as tmp:
            MODULE.MANAGER_CACHE_DIR = Path(tmp)
            MODULE.MANAGER_CACHE_TTL_SECONDS = 1
            report_date = datetime(2026, 5, 14, tzinfo=timezone.utc).date()
            payload = _minimal_management_payload(report_date)
            path = MODULE.management_cache_path("SHARKON2025", report_date)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(payload), encoding="utf-8")
            loaded = MODULE.load_management_cache("SHARKON2025", report_date)
        assert loaded["summary"]["users_count"] == 1
    finally:
        MODULE.MANAGER_CACHE_DIR = original_dir
        MODULE.MANAGER_CACHE_TTL_SECONDS = original_ttl


def test_build_management_trend_reuses_precomputed_anchor_payload():
    original_trend_days = MODULE.MANAGER_TREND_DAYS
    original_load = MODULE.load_management_cache
    original_fetch = MODULE.fetch_events_for_date
    original_aggregate = MODULE.aggregate_rows_with_intervals
    fetch_dates = []
    try:
        MODULE.MANAGER_TREND_DAYS = 3
        anchor = datetime(2026, 5, 14, tzinfo=timezone.utc).date()

        def fake_load(host, report_date):
            if report_date < anchor:
                return _minimal_management_payload(report_date)
            return None

        def fake_fetch(host, report_date):
            fetch_dates.append(report_date)
            return MODULE.get_report_bounds(report_date), []

        MODULE.load_management_cache = fake_load
        MODULE.fetch_events_for_date = fake_fetch
        MODULE.aggregate_rows_with_intervals = lambda events, start, end, host: []
        trend = MODULE.build_management_trend(
            "SHARKON2025",
            anchor,
            precomputed_payloads={anchor: _minimal_management_payload(anchor, users_count=2)},
        )
        assert [item["report_date"] for item in trend] == ["2026-05-12", "2026-05-13", "2026-05-14"]
        assert trend[-1]["users_count"] == 2
        assert fetch_dates == []
    finally:
        MODULE.MANAGER_TREND_DAYS = original_trend_days
        MODULE.load_management_cache = original_load
        MODULE.fetch_events_for_date = original_fetch
        MODULE.aggregate_rows_with_intervals = original_aggregate


def test_render_management_html_contains_action_queue():
    payload = {
        "generated_at_utc": "2026-05-14T12:00:00Z",
        "host": "SHARKON2025",
        "report_date": "2026-05-14",
        "report_timezone": "Europe/Moscow",
        "filters": {
            "owner": "Руководитель смены",
            "department": "Бухгалтерия",
        },
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
        "owner_rollups": [
            {
                "name": "Руководитель смены",
                "users_count": 1,
                "active_users": 0,
                "inactive_users": 1,
                "below_target_users": 0,
                "workday_total_active_seconds": 0,
                "workday_total_active_hhmm": "00:00",
                "portfolio_coverage_pct": 0.0,
                "actions_count": 1,
                "critical_actions_count": 1,
                "high_actions_count": 0,
                "medium_actions_count": 0,
                "low_actions_count": 0,
                "users": ["Иван Петров"],
            }
        ],
        "department_rollups": [
            {
                "name": "Бухгалтерия",
                "users_count": 1,
                "active_users": 0,
                "inactive_users": 1,
                "below_target_users": 0,
                "workday_total_active_seconds": 0,
                "workday_total_active_hhmm": "00:00",
                "portfolio_coverage_pct": 0.0,
                "actions_count": 1,
                "critical_actions_count": 1,
                "high_actions_count": 0,
                "medium_actions_count": 0,
                "low_actions_count": 0,
                "users": ["Иван Петров"],
            }
        ],
        "owner_roster": [
            {
                "name": "Руководитель смены",
                "display_name": "Сменный руководитель",
                "title": "Руководитель смены 1С",
                "department": "Операторы 1С",
                "contact": "@shift-lead",
                "escalation_to": "Финансовый директор",
                "notes": "Дневной контур",
                "users_count": 1,
                "active_users": 0,
                "inactive_users": 1,
                "below_target_users": 0,
                "workday_total_active_seconds": 0,
                "workday_total_active_hhmm": "00:00",
                "portfolio_coverage_pct": 0.0,
                "actions_count": 1,
                "critical_actions_count": 1,
                "high_actions_count": 0,
                "medium_actions_count": 0,
                "low_actions_count": 0,
                "users": ["Иван Петров"]
            }
        ],
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
    assert "По ответственным" in html
    assert "Ответственные и эскалация" in html
    assert "По подразделениям" in html
    assert "Свежесть источников данных" in html
    assert "Что делать сегодня" in html
    assert "Фильтр:" in html
    assert "Сбросить" in html
    assert "@shift-lead" in html
    assert "Иван Петров" in html
    assert "Руководитель смены" in html


def test_build_source_freshness_uses_host_fileops_bucket_only():
    original = MODULE.latest_bucket_event
    original_now_utc = MODULE.now_utc
    try:
        def fake_latest(bucket_id):
            if bucket_id == "aw-file-operations_SHARKON2025":
                return {
                    "timestamp": "2026-05-23T12:00:00Z",
                    "data": {"signalType": "collector_health", "queueDepth": 0, "sendFailures": 0, "eventsFlushed": 50},
                }
            return None

        MODULE.latest_bucket_event = fake_latest
        MODULE.now_utc = lambda: datetime(2026, 5, 23, 12, 5, 0, tzinfo=timezone.utc)
        sources, actions = MODULE.build_source_freshness("SHARKON2025")
        file_source = next(source for source in sources if source["source_id"] == "file_operations")
        assert file_source["bucket_id"] == "aw-file-operations_SHARKON2025"
        assert file_source["status"] == "ok"
        assert not any(action["evidence"].get("source_id") == "file_operations" for action in actions)
    finally:
        MODULE.latest_bucket_event = original
        MODULE.now_utc = original_now_utc


def test_send_bytes_returns_false_on_broken_pipe():
    handler = _FakeHandler(exc=BrokenPipeError())
    ok = MODULE.send_bytes(handler, b"{}", "application/json; charset=utf-8")
    assert ok is False
    assert handler.status == 200
    assert handler.headers["Content-Type"] == "application/json; charset=utf-8"
    assert handler.headers["Content-Length"] == "2"
    assert handler.ended is True


def test_send_bytes_writes_payload_when_client_is_connected():
    handler = _FakeHandler()
    ok = MODULE.send_bytes(handler, b"payload", "text/plain; charset=utf-8", status=201)
    assert ok is True
    assert handler.status == 201
    assert handler.headers["Content-Length"] == "7"
    assert handler.wfile.data == b"payload"
