#!/usr/bin/env python3
import importlib.util
from datetime import UTC, datetime
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("aw-dlp-influx-exporter.py")
SPEC = importlib.util.spec_from_file_location("aw_dlp_influx_exporter", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def test_build_endpoint_lines_emits_self_test_and_signal():
    events = [
        {
            "id": 10,
            "timestamp": "2026-05-15T10:00:00Z",
            "data": {
                "hostname": "SHARKON2025",
                "username": "Администратор",
                "signalType": "self_test",
                "policyMode": "server",
                "policySource": "local-fallback",
                "queueDepth": 2,
                "eventsEnqueued": 100,
                "eventsFlushed": 99,
                "sendFailures": 1,
                "policyEnabled": True,
            },
        },
        {
            "id": 11,
            "timestamp": "2026-05-15T10:01:00Z",
            "data": {
                "hostname": "SHARKON2025",
                "username": "Администратор",
                "signalType": "print_job",
                "source": "endpoint-signals-phase2",
                "documentName": "Документ.docx",
                "printerName": "HP LaserJet",
            },
        },
    ]
    lines = MODULE.build_endpoint_lines("SHARKON2025", events)
    assert any(line.startswith("aw_dlp_endpoint_self_test,") for line in lines)
    assert any(line.startswith("aw_dlp_signal,") for line in lines)


def test_normalize_incident_handles_nested_source_event():
    item = {
        "timestamp": "2026-05-15T10:02:00Z",
        "data": {
            "host": "SHARKON2025",
            "incident": {"status": "open", "verdict": "incident"},
            "sourceBucket": "aw-dlp-endpoint-signals_SHARKON2025",
            "sourceEvent": {
                "data": {
                    "signalType": "print_job",
                    "hostname": "SHARKON2025",
                    "username": "Администратор",
                    "documentName": "Письмо",
                    "printerName": "HP",
                    "source": "endpoint-signals-phase2",
                }
            },
        },
    }
    normalized = MODULE.normalize_incident(item, "SHARKON2025")
    assert normalized["signal_type"] == "print_job"
    assert normalized["username"] == "Администратор"
    assert normalized["action"] == "incident"
    assert normalized["incident_status"] == "open"


def test_build_case_lines_emits_case_state():
    cases = [
        {
            "id": 28,
            "host": "SHARKON2025",
            "status": "open",
            "severity": "medium",
            "assignee": None,
            "title": "DLP print_job · Администратор",
            "incident_id": "case-1",
            "evidence": {"items": [1], "chain_length": 1},
            "forensics": None,
            "updated_at": "2026-05-15T10:03:00+00:00",
        }
    ]
    lines = MODULE.build_case_lines("SHARKON2025", cases)
    assert len(lines) == 1
    assert lines[0].startswith("aw_dlp_case,")


def test_timestamp_parser_accepts_zulu():
    assert MODULE.pts("2026-05-15T10:00:00Z") == datetime(2026, 5, 15, 10, 0, 0, tzinfo=UTC)
