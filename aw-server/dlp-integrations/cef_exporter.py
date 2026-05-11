#!/usr/bin/env python3
from __future__ import annotations

import json
import logging
import os
import socket
from datetime import datetime, timezone

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")


def build_cef(event: dict) -> str:
    sev_map = {"low": 3, "medium": 6, "high": 10}
    sev = sev_map.get(event.get("severity", "low"), 3)
    ts = datetime.now(timezone.utc).isoformat()
    msg = event.get("message", "")
    host = event.get("hostname", "unknown")
    return f"CEF:0|AWatch-rus|DLP|1.0|{event.get('id','dlp')}|{msg}|{sev}|rt={ts} shost={host}"


def send_syslog(line: str, host: str, port: int) -> None:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        sock.sendto(line.encode("utf-8", errors="ignore"), (host, port))
    finally:
        sock.close()


def main() -> None:
    sample = os.environ.get("AW_DLP_CEF_SAMPLE", "")
    event = json.loads(sample) if sample else {"id": "startup", "message": "cef exporter heartbeat", "severity": "low"}
    line = build_cef(event)
    host = os.environ.get("AW_DLP_SYSLOG_HOST", "127.0.0.1")
    port = int(os.environ.get("AW_DLP_SYSLOG_PORT", "514"))
    send_syslog(line, host, port)
    logging.info("sent CEF event to %s:%d", host, port)


if __name__ == "__main__":
    main()
