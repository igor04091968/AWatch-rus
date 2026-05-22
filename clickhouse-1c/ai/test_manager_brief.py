#!/usr/bin/env python3
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import generate_manager_brief as gmb


class ManagerBriefTests(unittest.TestCase):
    def setUp(self) -> None:
        self.context = {
            "portfolio_summary": {
                "companies_total": 41,
                "critical_total": 10,
                "high_total": 20,
                "medium_total": 5,
                "low_total": 3,
                "none_total": 3,
                "direct_total": 33,
                "alias_total": 5,
                "manual_total": 3,
                "unmatched_total": 0,
                "stale_7d_total": 4,
                "stale_14d_total": 2,
                "busy_total": 7,
                "activity_30d_total": 12345.0,
                "activity_forecast_30d_total": 54321.0,
                "open_cases_total": 12,
                "detections_total": 20,
            },
            "freshness": [
                {"source": "documents_ts", "stale": False},
                {"source": "signals_ts", "stale": True},
            ],
            "top_risks": [
                {
                    "counterparty": "ФЕЛИЦТ ГРУПП 2026",
                    "signal_severity": "critical",
                    "top_signal": "Есть открытые кейсы",
                    "open_cases_total": 2,
                    "detections_total": 5,
                    "active_locks": 3,
                    "amount_30d": 100.0,
                    "amount_forecast_30d": 300.0,
                }
            ],
            "top_forecasts": [
                {
                    "counterparty": "АВКО 2026",
                    "amount_30d": 1000.0,
                    "amount_forecast_30d": 3000.0,
                    "registry_match_mode": "direct",
                    "signal_severity": "high",
                }
            ],
            "watchlist": [{"counterparty": "АВКО 2026"}],
        }

    def test_deterministic_payload_shape(self) -> None:
        payload = gmb.render_deterministic_payload(self.context)
        self.assertIn("headline", payload)
        self.assertGreaterEqual(len(payload["summary"]), 3)
        self.assertEqual(payload["top_risks"][0]["company"], "ФЕЛИЦТ ГРУПП 2026")
        self.assertIn("manual", " ".join(payload["caveats"]).lower())

    def test_markdown_render(self) -> None:
        payload = gmb.render_deterministic_payload(self.context)
        md = gmb.render_markdown(payload, "2026-05-22T12:00:00+00:00")
        self.assertIn("# Executive Brief 1C", md)
        self.assertIn("## Компании риска", md)
        self.assertIn("ФЕЛИЦТ ГРУПП 2026", md)


if __name__ == "__main__":
    unittest.main()
