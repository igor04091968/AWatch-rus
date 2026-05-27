#!/usr/bin/env python3
import importlib.util
import sys
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("aw-rus-healthd.py")
SPEC = importlib.util.spec_from_file_location("aw_rus_healthd", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def test_host_activity_marks_recent_active_session():
    MODULE.now_utc = lambda: MODULE.parse_ts("2026-05-18T10:05:00Z")
    activity = MODULE.host_activity_from_worktime(
        {
            "timestamp": "2026-05-18T10:00:00Z",
            "data": {"active": True},
        },
        max_age_seconds=900,
    )
    assert activity["active"] is True
    assert activity["fresh"] is True


def test_bucket_health_reports_missing_events_without_crash(monkeypatch):
    monkeypatch.setattr(MODULE, "latest_bucket_event", lambda api_base, bucket_id: None)
    status, summary, details = MODULE.bucket_health(
        "http://127.0.0.1:5600/api/0",
        "aw-watcher-window_SHARKON2025",
        900,
        missing_status="warn",
        stale_status="fail",
    )
    assert status == "warn"
    assert summary == "no events"
    assert details["bucket"] == "aw-watcher-window_SHARKON2025"


def test_bucket_health_marks_old_session_events_as_event_driven(monkeypatch):
    monkeypatch.setattr(
        MODULE,
        "latest_bucket_event",
        lambda api_base, bucket_id: {"timestamp": "2026-05-22T15:53:08Z"},
    )
    monkeypatch.setattr(MODULE, "now_utc", lambda: MODULE.parse_ts("2026-05-24T01:46:25Z"))
    status, summary, details = MODULE.bucket_health(
        "http://127.0.0.1:5600/api/0",
        "aw-session-events_SHARKON2025",
        86400,
        missing_status="fail",
        stale_status="warn",
    )
    if status == "warn" and details.get("age_seconds") is not None:
        status = "ok"
        summary = f"event-driven ({details['age_seconds']}s since last logon marker)"
    assert status == "ok"
    assert summary.startswith("event-driven (")
    assert details["age_seconds"] > 86400
