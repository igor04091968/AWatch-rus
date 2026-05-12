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

LOG = logging.getLogger("aw.dlp.cef_exporter")


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


def escape_cef(v: Any) -> str:
    s = "" if v is None else str(v)
    return s.replace("\\", "\\\\").replace("|", "\\|").replace("=", "\\=").replace("\n", "\\n").replace("\r", "")


def map_severity(name: str, mapping: dict[str, int]) -> int:
    return int(mapping.get((name or "").lower(), 3))


def build_cef(event: dict[str, Any], mapping: dict[str, int]) -> str:
    data = event.get("data") or {}
    sev_name = str(data.get("severity") or "low").lower()
    sev_num = map_severity(sev_name, mapping)
    rt = event.get("timestamp") or datetime.now(timezone.utc).isoformat()
    rule = data.get("ruleId") or "dlp-incident"
    msg = data.get("message") or "AWatch DLP incident"
    sig = data.get("signalType") or "unknown"
    host = data.get("hostname") or "unknown"
    user = data.get("username") or "unknown"
    action = data.get("action") or "alert"
    ext = (
        f"rt={escape_cef(rt)} "
        f"shost={escape_cef(host)} "
        f"suser={escape_cef(user)} "
        f"cs1Label=signalType cs1={escape_cef(sig)} "
        f"cs2Label=action cs2={escape_cef(action)} "
        f"cs3Label=ruleId cs3={escape_cef(rule)}"
    )
    return (
        f"CEF:0|AWatch-rus|DLP|1.0|{escape_cef(rule)}|{escape_cef(msg)}|{sev_num}|{ext}"
    )


def send_syslog_udp(line: str, host: str, port: int) -> None:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        sock.sendto(line.encode("utf-8", errors="ignore"), (host, port))
    finally:
        sock.close()


def send_syslog_tcp(line: str, host: str, port: int, timeout: int = 10) -> None:
    sock = socket.create_connection((host, port), timeout=timeout)
    try:
        sock.sendall((line + "\n").encode("utf-8", errors="ignore"))
    finally:
        sock.close()


def iter_new_incidents(
    aw_base: str,
    state: dict[str, Any],
    per_bucket_limit: int,
) -> tuple[list[dict[str, Any]], dict[str, int]]:
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


def main() -> None:
    setup_logging()
    cfg_path = Path("/opt/activitywatch/dlp-integrations/cef-config.yaml")
    cfg = load_yaml(cfg_path)
    aw_base = str(cfg.get("aw_api_base", "http://127.0.0.1:5600/api/0")).rstrip("/")
    syslog_host = str(cfg.get("syslog_host", "127.0.0.1"))
    syslog_port = int(cfg.get("syslog_port", 514))
    syslog_proto = str(cfg.get("syslog_proto", "udp")).lower()
    per_bucket_limit = int(cfg.get("per_bucket_limit", 300))
    state_path = Path(str(cfg.get("state_path", "/var/lib/activitywatch/dlp-integrations/cef-state.json")))
    sev_mapping = cfg.get("severity_mapping", {"low": 3, "medium": 6, "high": 10})
    if not isinstance(sev_mapping, dict):
        sev_mapping = {"low": 3, "medium": 6, "high": 10}

    state = load_json(state_path)
    incidents, max_ids = iter_new_incidents(aw_base=aw_base, state=state, per_bucket_limit=per_bucket_limit)
    sent = 0
    for ev in incidents:
        line = build_cef(ev, sev_mapping)
        if syslog_proto == "tcp":
            send_syslog_tcp(line, syslog_host, syslog_port)
        else:
            send_syslog_udp(line, syslog_host, syslog_port)
        sent += 1
    state["last_ids"] = max_ids
    state["updated_at"] = datetime.now(timezone.utc).isoformat()
    save_json(state_path, state)
    LOG.info("CEF exporter done: sent=%d buckets=%d target=%s:%d/%s", sent, len(max_ids), syslog_host, syslog_port, syslog_proto)


if __name__ == "__main__":
    main()
