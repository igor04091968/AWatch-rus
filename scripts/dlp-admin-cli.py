#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from urllib import request


def get_json(url: str):
    with request.urlopen(url, timeout=10) as r:
        return json.loads(r.read().decode("utf-8"))


def main() -> None:
    p = argparse.ArgumentParser(description="AWatch DLP admin CLI")
    p.add_argument("--server", default="http://127.0.0.1:5601")
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("policies-list")
    sub.add_parser("health-check")
    args = p.parse_args()

    if args.cmd == "policies-list":
        data = get_json(f"{args.server}/api/0/dlp/policies")
        print(json.dumps(data, ensure_ascii=False, indent=2))
    elif args.cmd == "health-check":
        data = get_json(f"{args.server}/health")
        print(json.dumps(data, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
