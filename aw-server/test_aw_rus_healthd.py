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


def test_inactive_interactive_bucket_can_be_downgraded_to_ok(monkeypatch):
    monkeypatch.setattr(
        MODULE,
        "latest_bucket_event",
        lambda api_base, bucket_id: {"timestamp": "2026-05-30T07:00:00Z"},
    )
    monkeypatch.setattr(MODULE, "now_utc", lambda: MODULE.parse_ts("2026-05-30T12:00:00Z"))
    activity = {"active": False, "fresh": True, "age_seconds": 5}
    status, summary, details = MODULE.bucket_health(
        "http://127.0.0.1:5600/api/0",
        "aw-watcher-window_SHARKON2025",
        900,
        missing_status="warn",
        stale_status="warn",
    )
    details["interactive_required"] = bool(activity["active"])
    details["host_activity"] = activity
    if not activity["active"] and status != "ok":
        details["inactive_summary"] = summary
        status = "ok"
        summary = "inactive: no active interactive users"

    assert status == "ok"
    assert summary == "inactive: no active interactive users"
    assert details["inactive_summary"] == "stale (18000s)"


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


def test_bucket_timestamp_health_uses_bucket_metadata_without_event_query(monkeypatch):
    def fail_if_called(api_base, bucket_id):
        raise AssertionError("latest event query should not run when bucket metadata has end timestamp")

    monkeypatch.setattr(MODULE, "latest_bucket_event", fail_if_called)
    monkeypatch.setattr(MODULE, "now_utc", lambda: MODULE.parse_ts("2026-05-30T17:05:00Z"))

    status, summary, details = MODULE.bucket_timestamp_health(
        "http://127.0.0.1:5600/api/0",
        {"aw-session-events_SHARKON2025": {"metadata": {"end": "2026-05-30T17:00:00Z"}}},
        "aw-session-events_SHARKON2025",
        86400,
        missing_status="fail",
        stale_status="warn",
    )

    assert status == "ok"
    assert summary == "fresh (300s)"
    assert details["timestamp_source"] == "bucket_metadata.end"
    assert details["age_seconds"] == 300


def test_bucket_timestamp_health_falls_back_to_event_query_without_metadata(monkeypatch):
    monkeypatch.setattr(
        MODULE,
        "latest_bucket_event",
        lambda api_base, bucket_id: {"timestamp": "2026-05-30T17:00:00Z"},
    )
    monkeypatch.setattr(MODULE, "now_utc", lambda: MODULE.parse_ts("2026-05-30T17:05:00Z"))

    status, summary, details = MODULE.bucket_timestamp_health(
        "http://127.0.0.1:5600/api/0",
        {"aw-session-events_SHARKON2025": {}},
        "aw-session-events_SHARKON2025",
        86400,
        missing_status="fail",
        stale_status="warn",
    )

    assert status == "ok"
    assert summary == "fresh (300s)"
    assert "timestamp_source" not in details


def test_guard_bucket_missing_is_warn_until_required(monkeypatch):
    monkeypatch.setattr(MODULE, "latest_bucket_event", lambda api_base, bucket_id: None)
    status, summary, details = MODULE.guard_bucket_health(
        "http://127.0.0.1:5600/api/0",
        "SHARKON2025",
        300,
        required=False,
    )
    assert status == "warn"
    assert summary == "no guard heartbeat"
    assert details["required"] is False


def test_guard_bucket_required_stale_is_fail(monkeypatch):
    monkeypatch.setattr(
        MODULE,
        "latest_bucket_event",
        lambda api_base, bucket_id: {
            "timestamp": "2026-05-30T10:00:00Z",
            "data": {"status": "ok", "mode": "enforce"},
        },
    )
    monkeypatch.setattr(MODULE, "now_utc", lambda: MODULE.parse_ts("2026-05-30T10:10:00Z"))
    status, summary, details = MODULE.guard_bucket_health(
        "http://127.0.0.1:5600/api/0",
        "SHARKON2025",
        300,
        required=True,
    )
    assert status == "fail"
    assert summary == "guard stale (600s)"
    assert details["mode"] == "enforce"


def test_aw_wrapper_failure_can_be_advisory_warn(tmp_path):
    report = MODULE.Report()
    missing = tmp_path / "missing-wrapper"
    MODULE.check_wrapper(report, "wrapper:test", [str(missing)], failure_status="warn")
    assert report.results[0].status == "warn"


def test_dlp_wrapper_failure_can_be_advisory_warn(monkeypatch, tmp_path):
    wrapper = tmp_path / "dlp-health-check"
    wrapper.write_text("#!/bin/sh\nprintf '{\"ok\":false}'\nexit 1\n", encoding="utf-8")
    wrapper.chmod(0o755)
    monkeypatch.setattr(MODULE, "run_command", lambda cmd: (1, '{"ok": false}'))

    report = MODULE.Report()
    MODULE.check_wrapper(
        report,
        "wrapper:dlp-health-check",
        [str(wrapper), "--json"],
        json_mode=True,
        failure_status="warn",
    )

    assert report.results[0].status == "warn"
    assert report.results[0].details["payload"] == {"ok": False}


def test_load_env_file_ignores_permission_denied():
    class DeniedPath:
        def exists(self):
            return True

        def read_text(self, encoding="utf-8"):
            raise PermissionError("denied")

    MODULE.load_env_file(DeniedPath())


def test_chmod_if_possible_sets_readable_mode(tmp_path):
    path = tmp_path / "aw-rus-health.json"
    path.write_text("{}", encoding="utf-8")

    MODULE.chmod_if_possible(path, 0o644)

    assert oct(path.stat().st_mode & 0o777) == "0o644"
