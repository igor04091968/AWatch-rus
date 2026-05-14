#!/usr/bin/env python3
import importlib.util
from datetime import date
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("aw-worktime-influx-exporter.py")
SPEC = importlib.util.spec_from_file_location("aw_worktime_influx_exporter", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def test_build_lines_for_day_emits_daily_hourly_and_summary(monkeypatch):
    bounds = MODULE.WORKTIME.get_report_bounds(date(2026, 5, 14))
    events = [
        {
            "timestamp": "2026-05-14T06:00:00Z",
            "duration": 0.0,
            "data": {
                "username": "user5",
                "userId": "WORKGROUP\\user5",
                "sessionId": 4,
                "state": "Активно",
                "active": True,
                "sampleSeconds": 1800,
            },
        }
    ]

    monkeypatch.setattr(MODULE.WORKTIME, "fetch_events_for_date", lambda host, report_date: (bounds, events))
    lines = MODULE.build_lines_for_day("SHARKON2025", date(2026, 5, 14))

    assert any(line.startswith("aw_rdp_worktime_daily,") for line in lines)
    assert any(line.startswith("aw_rdp_worktime_hourly,") for line in lines)
    assert any(line.startswith("aw_rdp_worktime_summary_daily,") for line in lines)
