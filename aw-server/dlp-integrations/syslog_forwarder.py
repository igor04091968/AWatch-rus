#!/usr/bin/env python3
from __future__ import annotations

import json
import logging
import socket
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib import error, request

import yaml

LOG = logging.getLogger("aw.dlp.syslog_forwarder")


def setup_logging() -> None:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")


def load_yaml(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    data = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    return data if isinstance(data, dict) else {}


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}
    return data if isinstance(data, dict) else {}


def save_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")


def http_json(url: str, timeout: int = 15) -> Any:
    req = request.Request(url, method="GET")
    with request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8", errors="ignore"))


def iter_new_incidents(aw_base: str, state: dict[str, Any], per_bucket_limit: int) -> tuple[list[dict[str, Any]], dict[str, int]]:
    buckets = http_json(f"{aw_base}/buckets/")
    bucket_ids = sorted([bid for bid in buckets.keys() if bid.startswith("aw-dlp-incidents_")])
    last_ids = state.get("last_ids", {})
    if not isinstance(last_ids, dict):
        last_ids = {}
    max_ids: dict[str, int] = {}
    out: list[dict[str, Any]] = []
    for bid in bucket_ids:
        try:
            events = http_json(f"{aw_base}/buckets/{bid}/events?limit={int(per_bucket_limit)}")
        except error.HTTPError as exc:
            LOG.warning("skip bucket %s: %s", bid, exc)
            continue
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


def build_message(event: dict[str, Any], app_name: str, facility: int) -> str:
    pri = facility * 8 + 6
    ts = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    data = event.get("data") or {}
    host = str(data.get("hostname") or "unknown")
    payload = json.dumps(
        {
            "event_id": event.get("id"),
            "timestamp": event.get("timestamp"),
            "host": host,
            "severity": data.get("severity"),
            "signalType": data.get("signalType"),
            "username": data.get("username"),
            "action": data.get("action"),
            "message": data.get("message"),
            "data": data,
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )
    return f"<{pri}>1 {ts} {host} {app_name} - - - {payload}"


def send_syslog(line: str, host: str, port: int, proto: str, timeout: int = 10) -> None:
    if proto.lower() == "tcp":
        sock = socket.create_connection((host, port), timeout=timeout)
        try:
            sock.sendall((line + "\n").encode("utf-8", errors="ignore"))
        finally:
            sock.close()
        return
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        sock.sendto(line.encode("utf-8", errors="ignore"), (host, port))
    finally:
        sock.close()


def main() -> None:
    setup_logging()
    cfg_path = Path("/opt/activitywatch/dlp-integrations/syslog-forwarder-config.yaml")
    cfg = load_yaml(cfg_path)
    aw_base = str(cfg.get("aw_api_base", "http://127.0.0.1:5600/api/0")).rstrip("/")
    state_path = Path(str(cfg.get("state_path", "/var/lib/activitywatch/dlp-integrations/syslog-forwarder-state.json")))
    per_bucket_limit = int(cfg.get("per_bucket_limit", 300))
    syslog_host = str(cfg.get("syslog_host", "127.0.0.1"))
    syslog_port = int(cfg.get("syslog_port", 514))
    syslog_proto = str(cfg.get("syslog_proto", "udp"))
    facility = int(cfg.get("facility", 16))
    app_name = str(cfg.get("app_name", "aw-dlp"))

    state = load_json(state_path)
    incidents, max_ids = iter_new_incidents(aw_base=aw_base, state=state, per_bucket_limit=per_bucket_limit)

    sent = 0
    for event in incidents:
        line = build_message(event, app_name=app_name, facility=facility)
        send_syslog(line=line, host=syslog_host, port=syslog_port, proto=syslog_proto)
        sent += 1

    save_json(state_path, {"last_ids": max_ids})
    LOG.info("syslog forwarder sent=%d buckets=%d", sent, len(max_ids))


if __name__ == "__main__":
    main()
