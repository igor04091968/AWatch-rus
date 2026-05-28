#!/usr/bin/env python3
import importlib.util
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("aw-worktime-ui-bridge.py")
SPEC = importlib.util.spec_from_file_location("aw_worktime_ui_bridge", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class WorktimeUiBridgeTests(unittest.TestCase):
    def test_bucket_needs_fallback_when_bucket_is_missing(self):
        now = datetime(2026, 5, 22, 16, 0, 0, tzinfo=timezone.utc)
        with mock.patch.object(MODULE, "get_latest_bucket_event_ts", return_value=None):
            self.assertTrue(MODULE.bucket_needs_fallback("aw-watcher-window_SHARKON2025", now, 600))

    def test_bucket_needs_fallback_when_bucket_is_recent(self):
        now = datetime(2026, 5, 22, 16, 0, 0, tzinfo=timezone.utc)
        recent = now - timedelta(seconds=120)
        with mock.patch.object(MODULE, "get_latest_bucket_event_ts", return_value=recent):
            self.assertFalse(MODULE.bucket_needs_fallback("aw-watcher-window_SHARKON2025", now, 600))

    def test_bucket_needs_fallback_when_bucket_is_stale(self):
        now = datetime(2026, 5, 22, 16, 0, 0, tzinfo=timezone.utc)
        stale = now - timedelta(seconds=601)
        with mock.patch.object(MODULE, "get_latest_bucket_event_ts", return_value=stale):
            self.assertTrue(MODULE.bucket_needs_fallback("aw-watcher-afk_SHARKON2025", now, 600))

    def test_get_latest_foreground_context_uses_recent_collector_health(self):
        now = datetime(2026, 5, 27, 8, 0, 0, tzinfo=timezone.utc)
        recent_events = [
            {
                "timestamp": "2026-05-27T07:59:30Z",
                "data": {
                    "signalType": "collector_health",
                    "foregroundProcess": "totalcmd",
                    "foregroundTitle": "Total Commander 6.01 - HARVEST",
                },
            }
        ]
        with mock.patch.object(MODULE, "_req", return_value=recent_events):
            ctx = MODULE.get_latest_foreground_context(now)
        self.assertEqual(ctx["app"], "totalcmd.exe")
        self.assertEqual(ctx["title"], "Total Commander 6.01 - HARVEST")

    def test_get_latest_active_session_ids_uses_latest_timestamp_group(self):
        events = [
            {
                "timestamp": "2026-05-27T07:59:25Z",
                "data": {"sessionId": 2, "state": "Активно", "username": "администратор", "sessionName": "console"},
            },
            {
                "timestamp": "2026-05-27T07:59:30Z",
                "data": {"sessionId": 2, "state": "Активно", "username": "администратор", "sessionName": "console"},
            },
            {
                "timestamp": "2026-05-27T07:59:30Z",
                "data": {"sessionId": 3, "state": "Активно", "username": "user5", "sessionName": "rdp-tcp#0"},
            },
            {
                "timestamp": "2026-05-27T07:59:30Z",
                "data": {"sessionId": 4, "state": "Диск", "username": "user1", "sessionName": ""},
            },
        ]
        self.assertEqual(MODULE.get_latest_active_session_ids(events), {2, 3})

    def test_get_latest_foreground_context_prefers_active_session(self):
        now = datetime(2026, 5, 27, 8, 0, 0, tzinfo=timezone.utc)
        recent_events = [
            {
                "timestamp": "2026-05-27T07:59:40Z",
                "data": {
                    "signalType": "collector_health",
                    "foregroundProcess": "explorer",
                    "foregroundTitle": "Explorer",
                    "sessionId": 9,
                },
            },
            {
                "timestamp": "2026-05-27T07:59:45Z",
                "data": {
                    "signalType": "collector_health",
                    "foregroundProcess": "totalcmd",
                    "foregroundTitle": "Total Commander 6.01 - HARVEST",
                    "sessionId": 2,
                },
            },
        ]
        with mock.patch.object(MODULE, "_req", return_value=recent_events):
            ctx = MODULE.get_latest_foreground_context(now, active_session_ids={2})
        self.assertEqual(ctx["app"], "totalcmd.exe")
        self.assertEqual(ctx["title"], "Total Commander 6.01 - HARVEST")

    def test_get_latest_foreground_context_uses_cached_context_when_recent_events_are_blank(self):
        now = datetime(2026, 5, 27, 8, 5, 0, tzinfo=timezone.utc)
        recent_events = [
            {
                "timestamp": "2026-05-27T08:04:50Z",
                "data": {
                    "signalType": "collector_health",
                    "foregroundProcess": "",
                    "foregroundTitle": "",
                    "sessionId": 3,
                },
            }
        ]
        state = {
            "last_foreground_context": {
                "app": "totalcmd.exe",
                "title": "Total Commander 6.01 - HARVEST",
                "timestamp": "2026-05-27T08:03:30Z",
            }
        }
        with mock.patch.object(MODULE, "_req", return_value=recent_events):
            ctx = MODULE.get_latest_foreground_context(now, active_session_ids={2, 3}, state=state)
        self.assertEqual(ctx["app"], "totalcmd.exe")
        self.assertEqual(ctx["title"], "Total Commander 6.01 - HARVEST")

    def test_transform_uses_foreground_context_for_active_sessions(self):
        events = [
            {
                "timestamp": "2026-05-27T07:59:30Z",
                "duration": 0,
                "data": {
                    "username": "user5",
                    "state": "Активно",
                    "sessionId": 3,
                    "sessionName": "rdp-tcp#0",
                },
            }
        ]
        afk_events, win_events, last_ts = MODULE.transform(
            events,
            foreground_context={"app": "totalcmd.exe", "title": "Total Commander 6.01 - HARVEST"},
        )
        self.assertEqual(afk_events[0]["data"]["status"], "not-afk")
        self.assertEqual(win_events[0]["data"]["app"], "totalcmd.exe")
        self.assertEqual(win_events[0]["data"]["title"], "Total Commander 6.01 - HARVEST")
        self.assertEqual(last_ts, "2026-05-27T07:59:30Z")

    def test_transform_caps_duration_at_next_timestamp_gap(self):
        events = [
            {
                "timestamp": "2026-05-27T07:59:30Z",
                "duration": 5,
                "data": {
                    "username": "user5",
                    "state": "Активно",
                    "sessionId": 3,
                    "sessionName": "rdp-tcp#0",
                },
            },
            {
                "timestamp": "2026-05-27T07:59:31Z",
                "duration": 5,
                "data": {
                    "username": "user5",
                    "state": "Активно",
                    "sessionId": 3,
                    "sessionName": "rdp-tcp#0",
                },
            },
        ]
        _, win_events, _ = MODULE.transform(
            events,
            foreground_context={"app": "totalcmd.exe", "title": "Total Commander 6.01 - HARVEST"},
        )
        self.assertEqual(win_events[0]["duration"], 1.0)
        self.assertEqual(win_events[1]["duration"], 5)

    def test_normalize_watcher_window_events_strips_rdp_suffix_for_real_apps(self):
        events = [
            {
                "timestamp": "2026-05-27T08:10:00Z",
                "duration": 5,
                "data": {
                    "app": "totalcmd.exe",
                    "title": "Total Commander 6.01 - HARVEST | RDP active (2): user5, администратор",
                    "source": "aw-worktime-ui-bridge",
                },
            },
            {
                "timestamp": "2026-05-27T08:10:05Z",
                "duration": 5,
                "data": {
                    "app": "RDP",
                    "title": "RDP active (2): user5, администратор",
                    "source": "aw-worktime-ui-bridge",
                },
            },
        ]
        normalized = MODULE.normalize_watcher_window_events(events)
        self.assertEqual(normalized[0]["data"]["title"], "Total Commander 6.01 - HARVEST")
        self.assertEqual(len(normalized), 1)

    def test_watcher_window_needs_bridge_sync_for_generic_bridge_rdp(self):
        now = datetime(2026, 5, 27, 8, 5, 0, tzinfo=timezone.utc)
        latest_event = {
            "timestamp": "2026-05-27T08:04:40Z",
            "data": {
                "app": "RDP",
                "title": "RDP active (2): user5, администратор",
                "source": "aw-worktime-ui-bridge",
            },
        }
        with mock.patch.object(MODULE, "get_latest_bucket_event", return_value=latest_event), \
             mock.patch.object(MODULE, "get_latest_bucket_event_ts", return_value=datetime(2026, 5, 27, 8, 4, 40, tzinfo=timezone.utc)):
            self.assertTrue(MODULE.watcher_window_needs_bridge_sync(now))

    def test_watcher_window_needs_bridge_sync_for_non_generic_bridge_event(self):
        now = datetime(2026, 5, 27, 8, 5, 0, tzinfo=timezone.utc)
        latest_event = {
            "timestamp": "2026-05-27T08:04:40Z",
            "data": {
                "app": "totalcmd.exe",
                "title": "Total Commander 6.01 - HARVEST",
                "source": "aw-worktime-ui-bridge",
            },
        }
        with mock.patch.object(MODULE, "get_latest_bucket_event", return_value=latest_event), \
             mock.patch.object(MODULE, "get_latest_bucket_event_ts", return_value=datetime(2026, 5, 27, 8, 4, 40, tzinfo=timezone.utc)):
            self.assertTrue(MODULE.watcher_window_needs_bridge_sync(now))

    def test_watcher_window_does_not_override_real_watcher_stream(self):
        now = datetime(2026, 5, 27, 8, 5, 0, tzinfo=timezone.utc)
        latest_event = {
            "timestamp": "2026-05-27T08:04:40Z",
            "data": {
                "app": "notepad.exe",
                "title": "Безымянный — Блокнот",
                "source": "aw-watcher-window",
            },
        }
        with mock.patch.object(MODULE, "get_latest_bucket_event", return_value=latest_event), \
             mock.patch.object(MODULE, "get_latest_bucket_event_ts", return_value=datetime(2026, 5, 27, 8, 4, 40, tzinfo=timezone.utc)):
            self.assertFalse(MODULE.watcher_window_needs_bridge_sync(now))


if __name__ == "__main__":
    unittest.main()
