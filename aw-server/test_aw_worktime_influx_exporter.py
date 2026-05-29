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
    monkeypatch.setattr(MODULE.WORKTIME, "build_true_active_apps", lambda host, report_date: [])
    lines = MODULE.build_lines_for_day("SHARKON2025", date(2026, 5, 14))

    assert any(line.startswith("aw_rdp_worktime_daily,") for line in lines)
    assert any(line.startswith("aw_rdp_worktime_hourly,") for line in lines)
    assert any(line.startswith("aw_rdp_worktime_summary_daily,") for line in lines)


def test_build_lines_for_day_emits_true_active_app_daily(monkeypatch):
    bounds = MODULE.WORKTIME.get_report_bounds(date(2026, 5, 14))

    monkeypatch.setattr(MODULE.WORKTIME, "fetch_events_for_date", lambda host, report_date: (bounds, []))
    monkeypatch.setattr(
        MODULE.WORKTIME,
        "build_true_active_apps",
        lambda host, report_date: [
            {
                "application": "1С",
                "proved_work_seconds": 2040,
                "proved_work_human": "34 мин",
                "proved_work_hhmm": "00:34",
                "last_action": "1С:Предприятие",
                "last_action_local": "09:42",
                "last_action_utc": "2026-05-14T06:42:00Z",
                "evidence_events": 7,
            }
        ],
    )

    lines = MODULE.build_lines_for_day("SHARKON2025", date(2026, 5, 14))

    true_active_line = next(line for line in lines if line.startswith("aw_true_active_app_daily,"))
    assert "application=1С" in true_active_line
    assert "report_date=2026-05-14" in true_active_line
    assert "proved_work_seconds=2040i" in true_active_line
    assert "evidence_events=7i" in true_active_line
    assert 'last_action="1С:Предприятие"' in true_active_line
