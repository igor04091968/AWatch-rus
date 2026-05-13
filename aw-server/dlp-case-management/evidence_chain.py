#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from typing import Any


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _canonical_json(payload: Any) -> str:
    return json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def evidence_sha256(payload: Any) -> str:
    return hashlib.sha256(_canonical_json(payload).encode("utf-8")).hexdigest()


def build_evidence_record(
    payload: Any,
    source_bucket: str | None = None,
    source_event_ts: str | None = None,
) -> dict[str, Any]:
    return {
        "recorded_at": _utc_now(),
        "source_bucket": source_bucket,
        "source_event_ts": source_event_ts,
        "sha256": evidence_sha256(payload),
        "payload": payload,
    }


def normalize_evidence_chain(
    payload: Any,
    source_bucket: str | None = None,
    source_event_ts: str | None = None,
) -> dict[str, Any]:
    if isinstance(payload, dict) and isinstance(payload.get("items"), list):
        return payload
    record = build_evidence_record(
        payload=payload,
        source_bucket=source_bucket,
        source_event_ts=source_event_ts,
    )
    return {
        "items": [record],
        "latest_sha256": record["sha256"],
        "chain_length": 1,
    }
