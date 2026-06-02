#!/usr/bin/env python3
import importlib.util
import sys
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("dlp-health-check.py")
SPEC = importlib.util.spec_from_file_location("dlp_health_check", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def test_endpoint_signals_ok_when_no_active_managed_hosts(monkeypatch):
    buckets = {
        "aw-worktime-sessions_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}},
        "aw-dlp-endpoint-signals_SHARKON2025": {"metadata": {"end": "2026-05-30T07:00:00Z"}},
    }
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:00:10Z"))
    monkeypatch.setattr(
        MODULE,
        "_http_json",
        lambda url: [{"timestamp": "2026-05-30T12:00:00Z", "data": {"active": False}}]
        if "aw-worktime-sessions_SHARKON2025/events" in url
        else [],
    )
    report = MODULE.HealthReport()

    MODULE.check_endpoint_signal_buckets(report, "http://127.0.0.1:5600/api/0", buckets, 900)

    result = report.results[0]
    assert result.name == "buckets:endpoint-signals"
    assert result.status == "ok"
    assert result.summary == "no active managed hosts require endpoint-signals freshness"
    assert result.details["ignored_inactive"] == ["aw-dlp-endpoint-signals_SHARKON2025"]


def test_endpoint_self_test_metrics_reports_transport_counters(monkeypatch):
    buckets = {"aw-dlp-endpoint-signals_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))
    monkeypatch.setattr(
        MODULE,
        "_http_json",
        lambda url, **kwargs: [
            {
                "timestamp": "2026-05-30T11:59:00Z",
                "data": {
                    "signalType": "self_test",
                    "queueDepth": 8,
                    "eventsEnqueued": 100,
                    "eventsFlushed": 92,
                    "sendFailures": 0,
                },
            },
            {
                "timestamp": "2026-05-30T12:00:30Z",
                "data": {
                    "signalType": "self_test",
                    "queueDepth": "3",
                    "eventsEnqueued": "120",
                    "eventsFlushed": "117",
                    "sendFailures": "0",
                },
            },
        ],
    )
    report = MODULE.HealthReport()

    MODULE.check_endpoint_self_test_metrics(report, "http://127.0.0.1:5600/api/0", buckets)

    result = report.results[0]
    assert result.name == "endpoint-self-test-metrics"
    assert result.status == "ok"
    latest = result.details["latest_self_tests"][0]
    assert latest["bucket"] == "aw-dlp-endpoint-signals_SHARKON2025"
    assert latest["timestamp"] == "2026-05-30T12:00:30Z"
    assert latest["age_seconds"] == 30
    assert latest["queueDepth"] == 3
    assert latest["eventsEnqueued"] == 120
    assert latest["eventsFlushed"] == 117
    assert latest["sendFailures"] == 0
    assert latest["sendFailuresDelta"] == 0


def test_endpoint_self_test_metrics_warns_on_transport_counters(monkeypatch):
    buckets = {"aw-dlp-endpoint-signals_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))
    monkeypatch.setattr(
        MODULE,
        "_http_json",
        lambda url, **kwargs: [
            {
                "timestamp": "2026-05-30T12:00:30Z",
                "data": {
                    "signalType": "self_test",
                    "queueDepth": 101,
                    "eventsEnqueued": 120,
                    "eventsFlushed": 19,
                    "sendFailures": 1,
                },
            }
        ],
    )
    report = MODULE.HealthReport()

    MODULE.check_endpoint_self_test_metrics(
        report,
        "http://127.0.0.1:5600/api/0",
        buckets,
        queue_warn_depth=100,
        send_failure_warn_count=1,
    )

    result = report.results[0]
    assert result.status == "warn"
    assert result.summary == "endpoint transport counters outside thresholds"
    assert result.details["warnings"] == [
        {"bucket": "aw-dlp-endpoint-signals_SHARKON2025", "metric": "queueDepth", "value": 101, "threshold": 100},
        {
            "bucket": "aw-dlp-endpoint-signals_SHARKON2025",
            "metric": "sendFailuresDelta",
            "value": 1,
            "current": 1,
            "previous": None,
            "threshold": 1,
        },
    ]


def test_file_operations_runtime_reports_health_and_latest_operations(monkeypatch):
    buckets = {"aw-file-operations_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))
    monkeypatch.setattr(
        MODULE,
        "_http_json",
        lambda url: [
            {
                "timestamp": "2026-05-30T12:00:30Z",
                "data": {
                    "signalType": "collector_health",
                    "queueDepth": "0",
                    "eventsEnqueued": "5",
                    "eventsFlushed": "5",
                    "sendFailures": "0",
                    "username": "USER1",
                    "hostname": "SHARKON2025",
                    "sessionId": "3",
                },
            },
            {
                "timestamp": "2026-05-30T12:00:20Z",
                "data": {
                    "operation": "Created",
                    "path": "C:\\Users\\USER1\\Downloads\\report.zip",
                    "extension": ".zip",
                    "archiveHint": True,
                    "username": "USER1",
                    "hostname": "SHARKON2025",
                    "size": "42",
                },
            },
        ],
    )
    report = MODULE.HealthReport()

    MODULE.check_file_operations_runtime(report, "http://127.0.0.1:5600/api/0", buckets)

    result = report.results[0]
    assert result.name == "file-operations-runtime"
    assert result.status == "ok"
    assert result.details["latest_health"][0]["queueDepth"] == 0
    assert result.details["latest_health"][0]["eventsEnqueued"] == 5
    assert result.details["latest_health"][0]["sendFailuresDelta"] == 0
    latest_op = result.details["latest_operations"][0]
    assert latest_op["operation"] == "Created"
    assert latest_op["extension"] == ".zip"
    assert latest_op["archiveHint"] is True
    assert latest_op["path_tail"] == "Downloads/report.zip"
    assert latest_op["size"] == 42
    assert result.details["sampled"][0]["operation_counts"] == {"Created": 1}


def test_file_operations_runtime_warns_on_transport_counters(monkeypatch):
    buckets = {"aw-file-operations_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))
    monkeypatch.setattr(
        MODULE,
        "_http_json",
        lambda url: [
            {
                "timestamp": "2026-05-30T12:00:30Z",
                "data": {
                    "signalType": "collector_health",
                    "queueDepth": 101,
                    "eventsEnqueued": 5,
                    "eventsFlushed": 2,
                    "sendFailures": 1,
                    "username": "USER1",
                    "hostname": "SHARKON2025",
                    "sessionId": 3,
                },
            }
        ],
    )
    report = MODULE.HealthReport()

    MODULE.check_file_operations_runtime(
        report,
        "http://127.0.0.1:5600/api/0",
        buckets,
        queue_warn_depth=100,
        send_failure_warn_count=1,
    )

    result = report.results[0]
    assert result.status == "warn"
    assert result.summary == "file-operations runtime counters outside expectations"
    assert result.details["warnings"] == [
        {"bucket": "aw-file-operations_SHARKON2025", "metric": "queueDepth", "value": 101, "threshold": 100},
        {
            "bucket": "aw-file-operations_SHARKON2025",
            "metric": "sendFailuresDelta",
            "value": 1,
            "current": 1,
            "previous": None,
            "threshold": 1,
        },
    ]


def test_endpoint_send_failures_uses_delta_baseline(monkeypatch):
    buckets = {"aw-dlp-endpoint-signals_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    state = {"counters": {}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))
    values = [12, 12, 13]

    def fake_http(url, **kwargs):
        value = values.pop(0)
        return [
            {
                "timestamp": "2026-05-30T12:00:30Z",
                "data": {
                    "signalType": "self_test",
                    "queueDepth": 0,
                    "eventsEnqueued": 120,
                    "eventsFlushed": 117,
                    "sendFailures": value,
                },
            }
        ]

    monkeypatch.setattr(MODULE, "_http_json", fake_http)

    first = MODULE.HealthReport()
    MODULE.check_endpoint_self_test_metrics(first, "http://127.0.0.1:5600/api/0", buckets, counter_state=state)
    assert first.results[0].status == "ok"
    assert first.results[0].details["latest_self_tests"][0]["sendFailuresPrevious"] is None
    assert first.results[0].details["latest_self_tests"][0]["sendFailuresDelta"] == 0

    second = MODULE.HealthReport()
    MODULE.check_endpoint_self_test_metrics(second, "http://127.0.0.1:5600/api/0", buckets, counter_state=state)
    assert second.results[0].status == "ok"
    assert second.results[0].details["latest_self_tests"][0]["sendFailuresPrevious"] == 12
    assert second.results[0].details["latest_self_tests"][0]["sendFailuresDelta"] == 0

    third = MODULE.HealthReport()
    MODULE.check_endpoint_self_test_metrics(third, "http://127.0.0.1:5600/api/0", buckets, counter_state=state)
    assert third.results[0].status == "warn"
    assert third.results[0].details["warnings"] == [
        {
            "bucket": "aw-dlp-endpoint-signals_SHARKON2025",
            "metric": "sendFailuresDelta",
            "value": 1,
            "current": 13,
            "previous": 12,
            "threshold": 1,
        }
    ]


def test_file_operations_send_failures_uses_delta_baseline(monkeypatch):
    buckets = {"aw-file-operations_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    state = {"counters": {}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))
    values = [28, 28, 29]

    def fake_http(url, **kwargs):
        value = values.pop(0)
        return [
            {
                "timestamp": "2026-05-30T12:00:30Z",
                "data": {
                    "signalType": "collector_health",
                    "queueDepth": 0,
                    "eventsEnqueued": 120,
                    "eventsFlushed": 117,
                    "sendFailures": value,
                },
            }
        ]

    monkeypatch.setattr(MODULE, "_http_json", fake_http)

    first = MODULE.HealthReport()
    MODULE.check_file_operations_runtime(first, "http://127.0.0.1:5600/api/0", buckets, counter_state=state)
    assert first.results[0].status == "ok"
    assert first.results[0].details["latest_health"][0]["sendFailuresPrevious"] is None
    assert first.results[0].details["latest_health"][0]["sendFailuresDelta"] == 0

    second = MODULE.HealthReport()
    MODULE.check_file_operations_runtime(second, "http://127.0.0.1:5600/api/0", buckets, counter_state=state)
    assert second.results[0].status == "ok"
    assert second.results[0].details["latest_health"][0]["sendFailuresPrevious"] == 28
    assert second.results[0].details["latest_health"][0]["sendFailuresDelta"] == 0

    third = MODULE.HealthReport()
    MODULE.check_file_operations_runtime(third, "http://127.0.0.1:5600/api/0", buckets, counter_state=state)
    assert third.results[0].status == "warn"
    assert third.results[0].details["warnings"] == [
        {
            "bucket": "aw-file-operations_SHARKON2025",
            "metric": "sendFailuresDelta",
            "value": 1,
            "current": 29,
            "previous": 28,
            "threshold": 1,
        }
    ]


def test_incident_runtime_reports_counts_and_latest_real_incidents(monkeypatch):
    buckets = {"aw-dlp-incidents_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))
    monkeypatch.setattr(
        MODULE,
        "_http_json",
        lambda url, **kwargs: [
            {
                "timestamp": "2026-05-30T12:00:30Z",
                "data": {
                    "ruleId": "usb-archive-copy",
                    "severity": "high",
                    "action": "alert",
                    "message": "Archive copied to removable device with a long message that should not leak full raw content.",
                    "username": "USER1",
                    "hostname": "SHARKON2025",
                    "source": "endpoint",
                },
            },
            {
                "timestamp": "2026-05-30T12:00:00Z",
                "data": {
                    "ruleId": "selftest-dlp-incident",
                    "severity": "low",
                    "action": "alert",
                    "message": "Self-test DLP incident from validation",
                    "signalType": "self_test",
                    "source": "self-test",
                },
            },
        ],
    )
    report = MODULE.HealthReport()

    MODULE.check_incident_runtime(report, "http://127.0.0.1:5600/api/0", buckets, sample_limit=20)

    result = report.results[0]
    assert result.name == "incident-runtime"
    assert result.status == "ok"
    assert result.summary == "1 real incidents in sampled events"
    assert result.details["totals"] == {"sampled_events": 2, "real_incidents": 1, "self_tests": 1}
    assert result.details["severity_counts"] == {"high": 1}
    assert result.details["action_counts"] == {"alert": 1}
    assert result.details["rule_counts"] == {"usb-archive-copy": 1}
    latest = result.details["latest_incidents"][0]
    assert latest["age_seconds"] == 30
    assert latest["ruleId"] == "usb-archive-copy"
    assert latest["message_excerpt"].endswith(".")


def test_incident_runtime_defaults_to_metadata_without_event_sampling(monkeypatch):
    buckets = {"aw-dlp-incidents_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}
    monkeypatch.setattr(MODULE, "_now_utc", lambda: MODULE._parse_ts("2026-05-30T12:01:00Z"))

    def fail_if_called(url, **kwargs):
        raise AssertionError("event sampling should stay disabled")

    monkeypatch.setattr(MODULE, "_http_json", fail_if_called)
    report = MODULE.HealthReport()

    MODULE.check_incident_runtime(report, "http://127.0.0.1:5600/api/0", buckets, sample_limit=0)

    result = report.results[0]
    assert result.name == "incident-runtime"
    assert result.status == "ok"
    assert result.summary == "incident event sampling disabled"
    assert result.details["metadata"] == [
        {
            "bucket": "aw-dlp-incidents_SHARKON2025",
            "end": "2026-05-30T12:00:00Z",
            "age_seconds": 60,
        }
    ]


def test_incident_runtime_warns_on_read_failure(monkeypatch):
    buckets = {"aw-dlp-incidents_SHARKON2025": {"metadata": {"end": "2026-05-30T12:00:00Z"}}}

    def fail_http(url, **kwargs):
        raise RuntimeError("timeout")

    monkeypatch.setattr(MODULE, "_http_json", fail_http)
    report = MODULE.HealthReport()

    MODULE.check_incident_runtime(report, "http://127.0.0.1:5600/api/0", buckets, sample_limit=20)

    result = report.results[0]
    assert result.name == "incident-runtime"
    assert result.status == "warn"
    assert result.summary == "1 incident buckets failed to sample"
    assert result.details["read_failed"] == [{"bucket": "aw-dlp-incidents_SHARKON2025", "error": "timeout"}]
