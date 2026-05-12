#!/usr/bin/env python3
from __future__ import annotations

import json
import logging
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib import error, request

import yaml

LOG = logging.getLogger("aw.dlp.webhook_sender")


def setup_logging() -> None:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")


def load_yaml(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    data = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    if not isinstance(data, dict):
        return {}
    return data


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        if isinstance(data, dict):
            return data
    except Exception:
        return {}
    return {}


def save_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")


def http_json(url: str, timeout: int = 15) -> Any:
    req = request.Request(url, method="GET")
    with request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8", errors="ignore"))


def post_with_retry(url: str, payload: dict[str, Any], retries: int, timeout: int, backoff_base: float) -> bool:
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json; charset=utf-8"}
    for attempt in range(1, retries + 1):
        try:
            req = request.Request(url, data=body, headers=headers, method="POST")
            with request.urlopen(req, timeout=timeout) as resp:
                code = getattr(resp, "status", 200)
                if 200 <= code < 300:
                    return True
        except error.HTTPError as exc:
            LOG.warning("webhook http error url=%s code=%s attempt=%d/%d", url, exc.code, attempt, retries)
        except Exception as exc:
            LOG.warning("webhook transport error url=%s err=%s attempt=%d/%d", url, exc, attempt, retries)
        if attempt < retries:
            time.sleep(backoff_base ** (attempt - 1))
    return False


def iter_new_incidents(aw_base: str, state: dict[str, Any], per_bucket_limit: int) -> tuple[list[dict[str, Any]], dict[str, int]]:
    buckets = http_json(f"{aw_base}/buckets/")
    bucket_ids = sorted([bid for bid in buckets.keys() if bid.startswith("aw-dlp-incidents_")])
    last_ids = state.get("last_ids", {})
    if not isinstance(last_ids, dict):
        last_ids = {}
    max_ids: dict[str, int] = {}
    out: list[dict[str, Any]] = []
    for bid in bucket_ids:
        events = http_json(f"{aw_base}/buckets/{bid}/events?limit={int(per_bucket_limit)}")
        prev = int(last_ids.get(bid, 0))
        bucket_max = prev
        for ev in events:
            eid = int(ev.get("id") or 0)
            if eid <= prev:
                continue
            out.append(ev)
            if eid > bucket_max:
                bucket_max = eid
        max_ids[bid] = bucket_max
    out.sort(key=lambda x: int(x.get("id") or 0))
    return out, max_ids


def should_send(severity: str, allowed: list[str]) -> bool:
    return severity.lower() in {s.lower() for s in allowed}


def main() -> None:
    setup_logging()
    cfg_path = Path("/opt/activitywatch/dlp-integrations/webhook-config.yaml")
    cfg = load_yaml(cfg_path)
    aw_base = str(cfg.get("aw_api_base", "http://127.0.0.1:5600/api/0")).rstrip("/")
    state_path = Path(str(cfg.get("state_path", "/var/lib/activitywatch/dlp-integrations/webhook-state.json")))
    retries = int(cfg.get("retries", 4))
    timeout = int(cfg.get("timeout_sec", 15))
    backoff_base = float(cfg.get("backoff_base", 2.0))
    per_bucket_limit = int(cfg.get("per_bucket_limit", 300))
    hooks = cfg.get("critical_webhooks", [])
    if not isinstance(hooks, list):
        hooks = []

    state = load_json(state_path)
    incidents, max_ids = iter_new_incidents(aw_base=aw_base, state=state, per_bucket_limit=per_bucket_limit)

    sent = 0
    for ev in incidents:
        data = ev.get("data") or {}
        severity = str(data.get("severity") or "low")
        for hook in hooks:
            if not isinstance(hook, dict):
                continue
            url = str(hook.get("url") or "").strip()
            if not url:
                continue
            allowed = hook.get("severity", ["high"])
            if isinstance(allowed, str):
                allowed = [allowed]
            if not should_send(severity, [str(x) for x in allowed]):
                continue
            payload = {
                "source": "AWatch-rus DLP",
                "timestamp": ev.get("timestamp"),
                "event_id": ev.get("id"),
                "severity": severity,
                "message": data.get("message"),
                "ruleId": data.get("ruleId"),
                "signalType": data.get("signalType"),
                "hostname": data.get("hostname"),
                "username": data.get("username"),
                "action": data.get("action"),
                "raw": data,
            }
            if post_with_retry(url=url, payload=payload, retries=retries, timeout=timeout, backoff_base=backoff_base):
                sent += 1

    state["last_ids"] = max_ids
    state["updated_at"] = datetime.now(timezone.utc).isoformat()
    save_json(state_path, state)
    LOG.info("Webhook sender done: delivered=%d incidents_seen=%d", sent, len(incidents))


if __name__ == "__main__":
    main()
