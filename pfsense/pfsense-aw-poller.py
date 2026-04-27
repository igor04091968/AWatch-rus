#!/usr/bin/env python3
import argparse
import json
import ssl
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


def utc_now_iso():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z"


def load_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def build_ssl_context(verify_tls):
    if verify_tls:
        return None
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    return context


def http_json(url, method="GET", headers=None, body=None, timeout=15, ssl_context=None):
    payload = None
    effective_headers = {"Content-Type": "application/json; charset=utf-8"}
    if headers:
        effective_headers.update(headers)
    if body is not None:
        payload = json.dumps(body).encode("utf-8")
    request = urllib.request.Request(url, data=payload, headers=effective_headers, method=method)
    with urllib.request.urlopen(request, timeout=timeout, context=ssl_context) as response:
        raw = response.read().decode("utf-8")
        return json.loads(raw) if raw else None


def ensure_bucket(aw_base_url, bucket_id, client_name, bucket_type, hostname, timeout, ssl_context):
    http_json(
        f"{aw_base_url}/buckets/{urllib.parse.quote(bucket_id, safe='')}",
        method="POST",
        body={
            "client": client_name,
            "type": bucket_type,
            "hostname": hostname,
        },
        timeout=timeout,
        ssl_context=ssl_context,
    )


def send_heartbeat(aw_base_url, bucket_id, event, pulse_time, timeout, ssl_context):
    http_json(
        f"{aw_base_url}/buckets/{urllib.parse.quote(bucket_id, safe='')}/heartbeat?pulsetime={pulse_time}",
        method="POST",
        body=event,
        timeout=timeout,
        ssl_context=ssl_context,
    )


def normalize_headers(config):
    headers = dict(config.get("headers") or {})
    auth = config.get("auth") or {}
    bearer_token = auth.get("bearer_token")
    basic = auth.get("basic")
    if bearer_token:
        headers["Authorization"] = f"Bearer {bearer_token}"
    elif basic and basic.get("username") and basic.get("password"):
        import base64
        token = base64.b64encode(f"{basic['username']}:{basic['password']}".encode("utf-8")).decode("ascii")
        headers["Authorization"] = f"Basic {token}"
    return headers


def summarize_payload(payload):
    if isinstance(payload, dict):
        return {
            "keys": sorted(payload.keys())[:50],
            "size": len(json.dumps(payload, ensure_ascii=False)),
        }
    if isinstance(payload, list):
        return {
            "items": len(payload),
            "sample_type": type(payload[0]).__name__ if payload else "none",
            "size": len(json.dumps(payload, ensure_ascii=False)),
        }
    return {
        "type": type(payload).__name__,
        "value": str(payload)[:400],
    }


def poll_endpoint(config, endpoint, aw_base_url, aw_timeout, pf_timeout, ssl_context):
    pf_host = config["pfsense"]["host"]
    scheme = config["pfsense"].get("scheme", "https")
    hostname = config["aw"]["hostname"]
    path = endpoint["path"]
    url = f"{scheme}://{pf_host}{path}"
    headers = normalize_headers(config["pfsense"])

    try:
        payload = http_json(
            url,
            method=endpoint.get("method", "GET"),
            headers=headers,
            timeout=pf_timeout,
            ssl_context=ssl_context,
        )
        bucket_prefix = endpoint["bucket_prefix"]
        bucket_id = f"{bucket_prefix}_{hostname}"
        ensure_bucket(
            aw_base_url,
            bucket_id,
            endpoint.get("client", bucket_prefix),
            endpoint.get("bucket_type", "aw.pfsense.metric"),
            hostname,
            aw_timeout,
            ssl_context,
        )
        event = {
            "timestamp": utc_now_iso(),
            "duration": 0,
            "data": {
                "source": "pfsense-aw-poller",
                "target": {
                    "host": pf_host,
                    "name": config["pfsense"].get("name", hostname),
                },
                "endpoint": {
                    "path": path,
                    "method": endpoint.get("method", "GET"),
                    "name": endpoint.get("name", bucket_prefix),
                },
                "summary": summarize_payload(payload),
                "payload": payload,
            },
        }
        send_heartbeat(
            aw_base_url,
            bucket_id,
            event,
            endpoint.get("pulse_time_seconds", config["aw"].get("pulse_time_seconds", 120)),
            aw_timeout,
            ssl_context,
        )
        return {
            "endpoint": path,
            "bucket_id": bucket_id,
            "status": "ok",
        }
    except urllib.error.HTTPError as error:
        return {
            "endpoint": path,
            "status": "http_error",
            "code": error.code,
            "reason": str(error),
        }
    except Exception as error:
        return {
            "endpoint": path,
            "status": "error",
            "reason": str(error),
        }


def run_once(config):
    aw = config["aw"]
    aw_base_url = f"http://{aw['server_host']}:{aw.get('server_port', 5600)}/api/0"
    aw_timeout = int(aw.get("timeout_seconds", 15))
    pf_timeout = int(config["pfsense"].get("timeout_seconds", 15))
    verify_tls = bool(config["pfsense"].get("verify_tls", False))
    ssl_context = build_ssl_context(verify_tls)

    results = []
    for endpoint in config.get("endpoints") or []:
        results.append(poll_endpoint(config, endpoint, aw_base_url, aw_timeout, pf_timeout, ssl_context))
    return results


def main():
    parser = argparse.ArgumentParser(description="Poll pfSense API and forward selected telemetry to ActivityWatch.")
    parser.add_argument("--config", required=True, help="Path to JSON config.")
    parser.add_argument("--once", action="store_true", help="Run one cycle and exit.")
    args = parser.parse_args()

    config = load_json(args.config)
    interval = int(config.get("poll_interval_seconds", 60))

    while True:
        results = run_once(config)
        print(json.dumps({"timestamp": utc_now_iso(), "results": results}, ensure_ascii=False), flush=True)
        if args.once:
            return 0
        time.sleep(max(interval, 15))


if __name__ == "__main__":
    sys.exit(main())
