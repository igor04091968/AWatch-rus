#!/usr/bin/env python3
import importlib.util
import sys
import tempfile
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("aw-slo-monitor.py")
SPEC = importlib.util.spec_from_file_location("aw_slo_monitor", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class FakeHeaders:
    def __init__(self, values):
        self.values = values

    def get(self, key, default=""):
        return self.values.get(key, default)


class FakeResponse:
    def __init__(self, body, status=200, content_type="text/plain"):
        self.body = body
        self.status = status
        self.headers = FakeHeaders({"Content-Type": content_type})

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self):
        return self.body


def test_summarize_window_calculates_9997_budget():
    now = MODULE.parse_ts("2026-05-30T12:00:00Z")
    samples = [
        {"ts": "2026-05-30T11:58:00Z", "ok": True},
        {"ts": "2026-05-30T11:59:00Z", "ok": False},
        {"ts": "2026-05-30T12:00:00Z", "ok": True},
    ]

    summary = MODULE.summarize_window(
        samples,
        now=now,
        window_seconds=86400,
        sample_interval_seconds=60,
        target_percent=99.97,
    )

    assert summary["samples"] == 3
    assert summary["good_samples"] == 2
    assert summary["bad_samples"] == 1
    assert summary["availability_percent"] == 66.66667
    assert summary["budget_seconds"] == 25
    assert summary["budget_remaining_seconds"] == -35
    assert summary["status"] == "burning"


def test_summarize_window_status_uses_remaining_error_budget_for_partial_window():
    now = MODULE.parse_ts("2026-05-30T12:00:00Z")
    samples = [
        {"ts": "2026-05-30T11:59:30Z", "ok": False},
        {"ts": "2026-05-30T11:59:45Z", "ok": True},
        {"ts": "2026-05-30T12:00:00Z", "ok": True},
    ]

    summary = MODULE.summarize_window(
        samples,
        now=now,
        window_seconds=86400,
        sample_interval_seconds=15,
        target_percent=99.97,
    )

    assert summary["availability_percent"] == 66.66667
    assert summary["budget_seconds"] == 25
    assert summary["budget_remaining_seconds"] == 10
    assert summary["status"] == "ok"


def test_append_and_trim_sample_keeps_retention_window():
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "samples.jsonl"
        path.write_text(
            '{"ts":"2026-05-01T00:00:00Z","ok":true}\n'
            '{"ts":"2026-05-30T11:59:00Z","ok":true}\n',
            encoding="utf-8",
        )
        sample = {"ts": "2026-05-30T12:00:00Z", "ok": False}

        samples = MODULE.append_and_trim_sample(path, sample, retention_seconds=3600)

        assert [item["ts"] for item in samples] == [
            "2026-05-30T11:59:00Z",
            "2026-05-30T12:00:00Z",
        ]
        assert "2026-05-01T00:00:00Z" not in path.read_text(encoding="utf-8")


def test_load_samples_treats_unreadable_file_as_empty(monkeypatch, tmp_path):
    path = tmp_path / "samples.jsonl"
    path.write_text('{"ts":"2026-05-30T12:00:00Z","ok":true}\n', encoding="utf-8")
    original_read_text = MODULE.Path.read_text

    def denied(self, *args, **kwargs):
        if self == path:
            raise PermissionError("denied")
        return original_read_text(self, *args, **kwargs)

    monkeypatch.setattr(MODULE.Path, "read_text", denied)

    assert MODULE.load_samples(path, MODULE.parse_ts("2026-05-30T11:00:00Z")) == []


def test_render_summary_text_includes_budget_remaining():
    summary = {
        "generated_at_utc": "2026-05-30T12:00:00Z",
        "target_percent": 99.97,
        "current_sample": {
            "ok": True,
            "probes": {"worktime_today_csv": {"ok": True, "status": 200}},
        },
        "windows": {
            "24h": {
                "status": "ok",
                "availability_percent": 100.0,
                "samples": 10,
                "bad_samples": 0,
                "observed_bad_seconds": 0,
                "budget_remaining_seconds": 25,
            }
        },
    }

    text = MODULE.render_summary_text(summary)

    assert "Target: 99.97%" in text
    assert "24h: ok availability=100.00000%" in text
    assert "budget_remaining_seconds=25" in text
    assert "- worktime_today_csv: OK 200" in text


def test_html_probe_requires_non_empty_html_with_markers(monkeypatch):
    body = b'<!doctype html><html><head><title>ActivityWatch</title></head><body><div id="app"></div><script src="/js/ru-patch-v5.js"></script></body></html>'
    monkeypatch.setattr(MODULE.request, "urlopen", lambda req, timeout: FakeResponse(body, content_type="text/html; charset=utf-8"))

    result = MODULE.html_probe(
        "http://127.0.0.1:5600/",
        5,
        min_bytes=100,
        required_markers=("ActivityWatch", 'id="app"', "ru-patch-v5.js"),
    )

    assert result["ok"] is True
    assert result["status"] == 200
    assert result["body_bytes"] == len(body)
    assert "body" not in result


def test_html_probe_fails_on_missing_marker(monkeypatch):
    monkeypatch.setattr(MODULE.request, "urlopen", lambda req, timeout: FakeResponse(b"<html></html>", content_type="text/html"))

    result = MODULE.html_probe(
        "http://127.0.0.1:5600/",
        5,
        min_bytes=1,
        required_markers=("ActivityWatch",),
    )

    assert result["ok"] is False
    assert result["missing_markers"] == ["ActivityWatch"]


def test_html_probe_retries_transient_fetch_error(monkeypatch):
    calls = {"count": 0}
    body = b"<html><body>ActivityWatch</body></html>"

    def fake_urlopen(req, timeout):
        calls["count"] += 1
        if calls["count"] == 1:
            raise TimeoutError("timed out")
        return FakeResponse(body, content_type="text/html")

    monkeypatch.setattr(MODULE.request, "urlopen", fake_urlopen)
    monkeypatch.setattr(MODULE.time, "sleep", lambda seconds: None)

    result = MODULE.html_probe(
        "http://127.0.0.1:5600/",
        5,
        min_bytes=1,
        required_markers=("ActivityWatch",),
    )

    assert result["ok"] is True
    assert result["attempts"] == 2
    assert calls["count"] == 2


def test_json_probe_validates_shape_and_expected_values(monkeypatch):
    body = b'{"generated_at_utc":"2026-05-30T12:00:00Z","host":"SHARKON2025","summary":{},"rows":[],"workday":{}}'
    monkeypatch.setattr(MODULE.request, "urlopen", lambda req, timeout: FakeResponse(body, content_type="application/json"))

    result = MODULE.json_probe(
        "http://127.0.0.1:5610/reports/worktime/management?format=json",
        5,
        expected_values={"host": "SHARKON2025"},
        required_keys=("generated_at_utc", "host", "summary", "rows", "workday"),
    )

    assert result["ok"] is True
    assert result["status"] == 200
    assert "body" not in result


def test_json_probe_fails_on_wrong_host(monkeypatch):
    body = b'{"generated_at_utc":"2026-05-30T12:00:00Z","host":"OTHER","summary":{},"rows":[],"workday":{}}'
    monkeypatch.setattr(MODULE.request, "urlopen", lambda req, timeout: FakeResponse(body, content_type="application/json"))

    result = MODULE.json_probe(
        "http://127.0.0.1:5610/reports/worktime/management?format=json",
        5,
        expected_values={"host": "SHARKON2025"},
        required_keys=("generated_at_utc", "host", "summary", "rows", "workday"),
    )

    assert result["ok"] is False
    assert result["mismatched_values"]["host"]["actual"] == "OTHER"


def test_load_env_file_ignores_permission_denied(monkeypatch):
    class DeniedPath:
        def exists(self):
            return True

        def read_text(self, encoding="utf-8"):
            raise PermissionError("denied")

    MODULE.load_env_file(DeniedPath())


def test_read_healthd_state_requires_fresh_ok_snapshot(monkeypatch, tmp_path):
    path = tmp_path / "aw-rus-health.json"
    path.write_text(
        '{"generated_at_utc":"2026-05-30T12:00:00Z","ok":true,"counts":{"ok":1,"warn":0,"fail":0}}',
        encoding="utf-8",
    )
    monkeypatch.setattr(MODULE, "now_utc", lambda: MODULE.parse_ts("2026-05-30T12:01:00Z"))

    fresh = MODULE.read_healthd_state(path, max_age_seconds=180)
    stale = MODULE.read_healthd_state(path, max_age_seconds=30)

    assert fresh["ok"] is True
    assert fresh["age_seconds"] == 60
    assert stale["ok"] is False


def test_chmod_if_possible_sets_readable_mode(tmp_path):
    path = tmp_path / "summary.json"
    path.write_text("{}", encoding="utf-8")

    MODULE.chmod_if_possible(path, 0o644)

    assert oct(path.stat().st_mode & 0o777) == "0o644"
