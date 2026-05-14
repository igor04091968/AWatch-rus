#!/usr/bin/env python3
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from case_storage import CaseStorage


class CaseStorageHayabusaLinkTest(unittest.TestCase):
    def test_link_hayabusa_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = Path(tmpdir) / "cases.db"
            storage = CaseStorage(db_path)
            created = storage.create_case(
                {
                    "incident_id": "inc-1",
                    "host": "SHARKON2025",
                    "title": "DLP print incident",
                    "severity": "high",
                },
                actor="test",
            )
            linked = storage.link_hayabusa(
                case_id=int(created["id"]),
                payload={
                    "host": "SHARKON2025",
                    "mode": "incident",
                    "status": "ok",
                    "intake_id": "pkg-1",
                    "report_dir": "/opt/hayabusa/reports/SHARKON2025/run-1",
                    "package_path": "/opt/hayabusa/archive/packages/SHARKON2025/pkg-1.zip",
                    "sha256": "abc123",
                    "link_source": "unit-test",
                },
                actor="test",
            )
            hayabusa = (linked.get("forensics") or {}).get("hayabusa") or {}
            self.assertEqual(hayabusa.get("tool"), "hayabusa")
            self.assertEqual(hayabusa.get("host"), "SHARKON2025")
            self.assertEqual(hayabusa.get("mode"), "incident")
            self.assertEqual(hayabusa.get("status"), "ok")
            self.assertEqual(hayabusa.get("intake_id"), "pkg-1")
            self.assertEqual(hayabusa.get("link_source"), "unit-test")
            audit = storage.list_audit(int(created["id"]))
            self.assertTrue(any(row.get("action") == "link_hayabusa" for row in audit))


if __name__ == "__main__":
    unittest.main()
