#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime, timedelta
from urllib import request
from urllib.parse import quote


def get_json(url: str):
    with request.urlopen(url, timeout=30) as r:
        return json.loads(r.read().decode("utf-8"))


def send_json(url: str, method: str, payload: dict | None = None):
    body = None if payload is None else json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = request.Request(url, data=body, method=method, headers={"Content-Type": "application/json"})
    with request.urlopen(req, timeout=30) as r:
        raw = r.read().decode("utf-8")
        return json.loads(raw) if raw else {}


def parse_iso(ts: str | None) -> datetime | None:
    if not ts:
        return None
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00")).astimezone(UTC)
    except ValueError:
        return None


def main() -> None:
    p = argparse.ArgumentParser(description="AWatch DLP admin CLI")
    p.add_argument("--policy-server", default="http://127.0.0.1:5601")
    p.add_argument("--case-server", default="http://127.0.0.1:5602")
    p.add_argument("--aw-server", default="http://127.0.0.1:5600")
    sub = p.add_subparsers(dest="cmd", required=True)

    policies = sub.add_parser("policies")
    policies_sub = policies.add_subparsers(dest="policies_cmd", required=True)
    policies_sub.add_parser("list")
    policies_sub.add_parser("active")

    incidents = sub.add_parser("incidents")
    incidents_sub = incidents.add_subparsers(dest="incidents_cmd", required=True)
    incidents_list = incidents_sub.add_parser("list")
    incidents_list.add_argument("--host")
    incidents_list.add_argument("--severity")
    incidents_list.add_argument("--limit", type=int, default=100)
    incidents_list.add_argument("--since-hours", type=int, default=24)

    cases = sub.add_parser("cases")
    cases_sub = cases.add_subparsers(dest="cases_cmd", required=True)
    cases_list = cases_sub.add_parser("list")
    cases_list.add_argument("--host")
    cases_list.add_argument("--status")
    cases_list.add_argument("--limit", type=int, default=100)
    cases_create = cases_sub.add_parser("create")
    cases_create.add_argument("--incident-id", required=True)
    cases_create.add_argument("--title", required=True)
    cases_create.add_argument("--host")
    cases_create.add_argument("--severity", default="medium")

    health = sub.add_parser("health")
    health_sub = health.add_subparsers(dest="health_cmd", required=True)
    health_sub.add_parser("check")

    args = p.parse_args()

    if args.cmd == "policies" and args.policies_cmd == "list":
        data = get_json(f"{args.policy_server}/api/0/dlp/policies")
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return

    if args.cmd == "policies" and args.policies_cmd == "active":
        data = get_json(f"{args.policy_server}/api/0/dlp/policies/active")
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return

    if args.cmd == "incidents" and args.incidents_cmd == "list":
        bucket_map = get_json(f"{args.aw_server}/api/0/buckets")
        if not isinstance(bucket_map, dict):
            print("[]")
            return

        bucket_ids = [x for x in bucket_map.keys() if str(x).startswith("aw-dlp-incidents_")]
        if args.host:
            bucket_ids = [x for x in bucket_ids if str(x).endswith("_" + args.host)]

        after = datetime.now(UTC) - timedelta(hours=max(1, args.since_hours))
        rows = []
        for bucket_id in sorted(bucket_ids):
            encoded = quote(str(bucket_id), safe="")
            events = get_json(f"{args.aw_server}/api/0/buckets/{encoded}/events?limit={max(1, args.limit)}")
            if not isinstance(events, list):
                continue
            for ev in events:
                if not isinstance(ev, dict):
                    continue
                ts = parse_iso(ev.get("timestamp"))
                if ts is None or ts < after:
                    continue
                data = ev.get("data") or {}
                if args.severity and str((data or {}).get("severity", "")).lower() != args.severity.lower():
                    continue
                rows.append(ev)
        print(json.dumps(rows, ensure_ascii=False, indent=2))
        return

    if args.cmd == "cases" and args.cases_cmd == "list":
        query = []
        if args.host:
            query.append(f"host={quote(args.host, safe='')}")
        if args.status:
            query.append(f"status={quote(args.status, safe='')}")
        query.append(f"limit={max(1, args.limit)}")
        data = get_json(f"{args.case_server}/api/0/dlp/cases?{'&'.join(query)}")
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return

    if args.cmd == "cases" and args.cases_cmd == "create":
        payload = {
            "incident_id": args.incident_id,
            "title": args.title,
            "host": args.host,
            "severity": args.severity,
            "evidence": {"source": "dlp-admin-cli"},
        }
        data = send_json(f"{args.case_server}/api/0/dlp/cases", "POST", payload)
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return

    if args.cmd == "health" and args.health_cmd == "check":
        out = {}
        try:
            out["policy"] = get_json(f"{args.policy_server}/healthz")
        except Exception as exc:
            out["policy"] = {"status": "error", "error": str(exc)}

        try:
            out["cases"] = get_json(f"{args.case_server}/health")
        except Exception as exc:
            out["cases"] = {"status": "error", "error": str(exc)}

        try:
            out["aw"] = get_json(f"{args.aw_server}/api/0/info")
        except Exception as exc:
            out["aw"] = {"status": "error", "error": str(exc)}

        print(json.dumps(out, ensure_ascii=False, indent=2))
        return

    raise SystemExit("unsupported command")


if __name__ == "__main__":
    main()
