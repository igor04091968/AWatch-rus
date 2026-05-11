from __future__ import annotations

from typing import Any


def build_policy_bundle(record: dict[str, Any] | None) -> dict[str, Any]:
    if not record:
        return {
            "active": False,
            "policyId": None,
            "name": None,
            "version": None,
            "checksum": None,
            "updatedAtUtc": None,
            "policy": None,
        }

    return {
        "active": True,
        "policyId": record["id"],
        "name": record["name"],
        "version": record["current_version"],
        "checksum": record["checksum"],
        "updatedAtUtc": record["updated_at"],
        "policy": record["policy"],
    }
