#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import time
from urllib import request


def post(url: str, payload: dict, retries: int = 3) -> bool:
    body = json.dumps(payload).encode("utf-8")
    for i in range(retries):
        try:
            req = request.Request(url, data=body, headers={"Content-Type": "application/json"}, method="POST")
            with request.urlopen(req, timeout=10):
                return True
        except Exception:
            time.sleep(2 ** i)
    return False


def main() -> None:
    hooks = [h.strip() for h in os.environ.get("AW_DLP_CRITICAL_WEBHOOKS", "").split(",") if h.strip()]
    payload = {"text": "AWatch DLP critical incident", "severity": "high"}
    for hook in hooks:
        post(hook, payload)


if __name__ == "__main__":
    main()
