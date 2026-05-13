#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json

from report_generator import generate_report


def main() -> None:
    parser = argparse.ArgumentParser(description="Run one or more DLP compliance report profiles")
    parser.add_argument("--month", help="Month in YYYY-MM format")
    parser.add_argument("--profiles", default="152-fz,pci-dss", help="Comma-separated profiles to generate")
    parser.add_argument("--stdout-json", action="store_true")
    args = parser.parse_args()

    profiles = [item.strip() for item in str(args.profiles).split(",") if item.strip()]
    results = [generate_report(month=args.month, profile=profile) for profile in profiles]
    if args.stdout_json:
        print(json.dumps({"items": results}, ensure_ascii=False))


if __name__ == "__main__":
    main()
