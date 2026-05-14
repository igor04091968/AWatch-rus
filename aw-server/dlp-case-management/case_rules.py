#!/usr/bin/env python3
from __future__ import annotations


def is_self_test_case(incident_id: str | None, title: str | None) -> bool:
    incident = str(incident_id or "").lower()
    label = str(title or "").lower()
    return "|self_test|" in incident or label.startswith("dlp self_test")
