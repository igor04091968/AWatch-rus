#!/usr/bin/env python3
from __future__ import annotations

import unittest

from case_rules import is_self_test_case


class CaseRulesTest(unittest.TestCase):
    def test_is_self_test_case_by_incident_id(self) -> None:
        self.assertTrue(is_self_test_case("2026-05-14T17:00:52.186Z|self_test|Администратор|||", "Normal"))

    def test_is_self_test_case_by_title(self) -> None:
        self.assertTrue(is_self_test_case("inc-1", "DLP self_test · Администратор"))

    def test_is_self_test_case_false_for_normal_case(self) -> None:
        self.assertFalse(is_self_test_case("inc-1", "DLP print incident"))


if __name__ == "__main__":
    unittest.main()
