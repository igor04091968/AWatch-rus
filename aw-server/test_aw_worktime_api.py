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
