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


if __name__ == "__main__":
    unittest.main()
