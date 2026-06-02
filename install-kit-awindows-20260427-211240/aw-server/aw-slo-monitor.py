#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import time
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any
from urllib import request


ENV_FILE = Path("/etc/activitywatch/aw-server.env")
DEFAULT_STATE_DIR = Path("/var/lib/activitywatch/slo")
DEFAULT_HEALTHD_CMD = "/usr/local/bin/aw-rus-healthd.py --json"
DEFAULT_HEALTHD_STATE_FILE = Path("/var/lib/activitywatch/health/aw-rus-health.json")


def load_env_file(path: Path) -> None:
    if not path.exists():
        return
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except PermissionError:
        # systemd EnvironmentFile has already loaded the variables for the service.
        return
    for raw_line in lines:
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip("'").strip('"'))


def env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


def now_utc() -> datetime:
    return datetime.now(UTC)


def parse_ts(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(UTC)


def iso(dt: datetime) -> str:
    return dt.astimezone(UTC).isoformat().replace("+00:00", "Z")


def write_atomic(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as handle:
        handle.write(content)
        tmp_name = handle.name
    os.replace(tmp_name, path)


def chmod_if_possible(path: Path, mode: int) -> None:
    try:
        path.chmod(mode)
    except OSError:
        pass


def chown_like_parent_if_possible(path: Path) -> None:
    try:
        parent_stat = path.parent.stat()
        os.chown(path, parent_stat.st_uid, parent_stat.st_gid)
    except OSError:
        pass


def fetch_url(url: str, timeout_seconds: float, *, accept: str | None = None, attempts: int = 1) -> dict[str, Any]:
    started = now_utc()
    last_error = ""
    for attempt in range(1, max(1, attempts) + 1):
        try:
            req = request.Request(url, headers={"Accept": accept}) if accept else url
            with request.urlopen(req, timeout=timeout_seconds) as resp:
                body = resp.read()
                status = int(resp.status)
                content_type = resp.headers.get("Content-Type", "")
            ok = 200 <= status < 300
            return {
                "ok": ok,
                "status": status,
                "body_bytes": len(body),
                "content_type": content_type,
                "body": body,
                "attempts": attempt,
                "latency_ms": int((now_utc() - started).total_seconds() * 1000),
                "url": url,
            }
        except Exception as exc:
            last_error = str(exc)
            if attempt < max(1, attempts):
                time.sleep(1)
    return {
        "ok": False,
        "error": last_error,
        "body_bytes": 0,
        "attempts": max(1, attempts),
        "latency_ms": int((now_utc() - started).total_seconds() * 1000),
        "url": url,
    }


def public_probe_result(result: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in result.items() if key != "body"}


def http_probe(url: str, timeout_seconds: float) -> dict[str, Any]:
    return public_probe_result(fetch_url(url, timeout_seconds, attempts=2))


def html_probe(
    url: str,
    timeout_seconds: float,
    *,
    min_bytes: int,
    required_markers: tuple[str, ...],
) -> dict[str, Any]:
    result = fetch_url(url, timeout_seconds, accept="text/html", attempts=2)
    if not result.get("ok"):
        return public_probe_result(result)

    body = result.get("body", b"")
    text = body.decode("utf-8", errors="replace") if isinstance(body, bytes) else str(body)
    missing = [marker for marker in required_markers if marker not in text]
    content_type = str(result.get("content_type") or "")
    if len(body) < min_bytes:
        result["ok"] = False
        result["error"] = f"body too small: {len(body)} < {min_bytes}"
    elif "text/html" not in content_type.lower():
        result["ok"] = False
        result["error"] = f"unexpected content-type: {content_type or 'unknown'}"
    elif missing:
        result["ok"] = False
        result["error"] = "missing markers: " + ", ".join(missing)
        result["missing_markers"] = missing
    return public_probe_result(result)


def json_probe(
    url: str,
    timeout_seconds: float,
    *,
    expected_values: dict[str, Any],
    required_keys: tuple[str, ...],
) -> dict[str, Any]:
    result = fetch_url(url, timeout_seconds, accept="application/json", attempts=2)
    if not result.get("ok"):
        return public_probe_result(result)

    body = result.get("body", b"")
    try:
        payload = json.loads(body.decode("utf-8") if isinstance(body, bytes) else str(body))
    except Exception as exc:
        result["ok"] = False
        result["error"] = f"invalid json: {exc}"
        return public_probe_result(result)

    if not isinstance(payload, dict):
        result["ok"] = False
        result["error"] = "json root is not object"
        return public_probe_result(result)

    missing_keys = [key for key in required_keys if key not in payload]
    mismatched = {
        key: {"expected": expected, "actual": payload.get(key)}
        for key, expected in expected_values.items()
        if payload.get(key) != expected
    }
    if missing_keys:
        result["ok"] = False
        result["error"] = "missing json keys: " + ", ".join(missing_keys)
        result["missing_keys"] = missing_keys
    elif mismatched:
        result["ok"] = False
        result["error"] = "unexpected json values"
        result["mismatched_values"] = mismatched
    else:
        result["json_keys"] = sorted(payload.keys())
    return public_probe_result(result)


def run_healthd(command: str, timeout_seconds: int) -> dict[str, Any]:
    try:
        proc = subprocess.run(
            command,
            shell=True,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired as exc:
        return {
            "ok": False,
            "returncode": None,
            "error": f"timeout after {timeout_seconds}s",
            "output_tail": (exc.stdout or "")[-1000:] if isinstance(exc.stdout, str) else "",
        }

    payload: dict[str, Any] = {}
    try:
        payload = json.loads(proc.stdout or "{}")
    except json.JSONDecodeError:
        pass
    return {
        "ok": proc.returncode == 0 and bool(payload.get("ok")),
        "returncode": proc.returncode,
        "counts": payload.get("counts", {}),
        "payload": payload if isinstance(payload, dict) else {},
        "output_tail": (proc.stdout or "")[-1000:],
    }


def read_healthd_state(path: Path, max_age_seconds: int) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        generated = parse_ts(str(payload["generated_at_utc"]))
        age = max(0, int((now_utc() - generated).total_seconds()))
    except Exception as exc:
        return {"ok": False, "error": str(exc), "path": str(path)}
    return {
        "ok": bool(payload.get("ok")) and age <= max_age_seconds,
        "counts": payload.get("counts", {}),
        "age_seconds": age,
        "path": str(path),
        "payload": payload if isinstance(payload, dict) else {},
    }


def load_samples(path: Path, cutoff: datetime) -> list[dict[str, Any]]:
    samples: list[dict[str, Any]] = []
    if not path.exists():
        return samples
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except PermissionError:
        return samples
    for line in lines:
        if not line.strip():
            continue
        try:
            item = json.loads(line)
            ts = parse_ts(str(item["ts"]))
        except Exception:
            continue
        if ts >= cutoff:
            samples.append(item)
    return samples


def append_and_trim_sample(path: Path, sample: dict[str, Any], retention_seconds: int) -> list[dict[str, Any]]:
    cutoff = parse_ts(sample["ts"]) - timedelta(seconds=retention_seconds)
    samples = load_samples(path, cutoff)
    samples.append(sample)
    content = "".join(json.dumps(item, ensure_ascii=False, separators=(",", ":")) + "\n" for item in samples)
    write_atomic(path, content)
    chmod_if_possible(path, 0o644)
    chown_like_parent_if_possible(path)
    return samples


def summarize_window(
    samples: list[dict[str, Any]],
    *,
    now: datetime,
    window_seconds: int,
    sample_interval_seconds: int,
    target_percent: float,
) -> dict[str, Any]:
    cutoff = now - timedelta(seconds=window_seconds)
    window = []
    for item in samples:
        try:
            if parse_ts(str(item["ts"])) >= cutoff:
                window.append(item)
        except Exception:
            continue

    total = len(window)
    good = sum(1 for item in window if item.get("ok") is True)
    bad = total - good
    availability = round((good / total) * 100, 5) if total else None
    observed_bad_seconds = bad * sample_interval_seconds
    budget_seconds = int(window_seconds * ((100.0 - target_percent) / 100.0))
    budget_remaining_seconds = budget_seconds - observed_bad_seconds
    status = "unknown"
    if total:
        status = "ok" if budget_remaining_seconds >= 0 else "burning"
    return {
        "window_seconds": window_seconds,
        "samples": total,
        "good_samples": good,
        "bad_samples": bad,
        "availability_percent": availability,
        "target_percent": target_percent,
        "observed_bad_seconds": observed_bad_seconds,
        "budget_seconds": budget_seconds,
        "budget_remaining_seconds": budget_remaining_seconds,
        "status": status,
    }


def render_summary_text(summary: dict[str, Any]) -> str:
    lines = [
        "=== AW-RUS SLO ===",
        f"Timestamp: {summary['generated_at_utc']}",
        f"Target: {summary['target_percent']}%",
        "",
    ]
    for name, data in summary["windows"].items():
        availability = data["availability_percent"]
        availability_text = "n/a" if availability is None else f"{availability:.5f}%"
        remaining = data["budget_remaining_seconds"]
        lines.append(
            f"{name}: {data['status']} availability={availability_text} "
            f"samples={data['samples']} bad={data['bad_samples']} "
            f"bad_seconds={data['observed_bad_seconds']} budget_remaining_seconds={remaining}"
        )
    lines.append("")
    lines.append(f"Current sample: {'OK' if summary['current_sample']['ok'] else 'FAIL'}")
    for name, probe in summary["current_sample"].get("probes", {}).items():
        marker = "OK" if probe.get("ok") else "FAIL"
        detail = probe.get("status", probe.get("error", ""))
        lines.append(f"- {name}: {marker} {detail}")
    return "\n".join(lines)


def build_sample(args: argparse.Namespace) -> dict[str, Any]:
    ts = iso(now_utc())
    if args.healthd_mode == "run":
        health = run_healthd(args.healthd_cmd, args.health_timeout_seconds)
    else:
        health = read_healthd_state(Path(args.healthd_state_file), args.healthd_state_max_age_seconds)
    probes = {
        "aw_webui_index": html_probe(
            args.aw_webui_url,
            args.http_timeout_seconds,
            min_bytes=1000,
            required_markers=("ActivityWatch", 'id="app"', "ru-patch-v5.js"),
        ),
        "worktime_today_html": html_probe(
            args.worktime_today_html_url,
            args.http_timeout_seconds,
            min_bytes=5000,
            required_markers=("AW-rus", "<html", "</html>"),
        ),
        "worktime_management_html": html_probe(
            args.worktime_management_html_url,
            args.http_timeout_seconds,
            min_bytes=5000,
            required_markers=("AW-rus", "<html", "</html>"),
        ),
        "worktime_today_csv": http_probe(args.worktime_today_csv_url, args.http_timeout_seconds),
        "worktime_management_json": json_probe(
            args.worktime_management_json_url,
            args.http_timeout_seconds,
            expected_values={"host": args.host},
            required_keys=("generated_at_utc", "host", "summary", "rows", "workday"),
        ),
    }
    ok = bool(health.get("ok")) and all(probe.get("ok") for probe in probes.values())
    return {
        "ts": ts,
        "ok": ok,
        "healthd_ok": bool(health.get("ok")),
        "healthd_counts": health.get("counts", {}),
        "probes": probes,
    }


def main() -> int:
    load_env_file(ENV_FILE)
    parser = argparse.ArgumentParser(description="AW-RUS rolling SLO sampler")
    parser.add_argument("--state-dir", default=env("AW_RUS_SLO_STATE_DIR", str(DEFAULT_STATE_DIR)))
    parser.add_argument("--healthd-cmd", default=env("AW_RUS_SLO_HEALTHD_CMD", DEFAULT_HEALTHD_CMD))
    parser.add_argument("--healthd-mode", choices=["state", "run"], default=env("AW_RUS_SLO_HEALTHD_MODE", "state"))
    parser.add_argument("--healthd-state-file", default=env("AW_RUS_SLO_HEALTHD_STATE_FILE", str(DEFAULT_HEALTHD_STATE_FILE)))
    parser.add_argument("--healthd-state-max-age-seconds", type=int, default=int(env("AW_RUS_SLO_HEALTHD_STATE_MAX_AGE_SECONDS", "180")))
    parser.add_argument("--aw-base", default=env("AW_RUS_SLO_AW_BASE", env("AW_SERVER_URL", "http://127.0.0.1:5600")))
    parser.add_argument("--worktime-base", default=env("AW_RUS_SLO_WORKTIME_BASE", env("AW_RUS_HEALTH_WORKTIME_API", "http://127.0.0.1:5610")))
    parser.add_argument("--host", default=env("AW_RUS_SLO_HOST", env("AW_MONITORED_WINDOWS_HOSTNAME", "SHARKON2025")))
    parser.add_argument("--target-percent", type=float, default=float(env("AW_RUS_SLO_TARGET_PERCENT", "99.97")))
    parser.add_argument("--sample-interval-seconds", type=int, default=int(env("AW_RUS_SLO_SAMPLE_INTERVAL_SECONDS", "15")))
    parser.add_argument("--retention-days", type=int, default=int(env("AW_RUS_SLO_RETENTION_DAYS", "35")))
    parser.add_argument("--http-timeout-seconds", type=float, default=float(env("AW_RUS_SLO_HTTP_TIMEOUT_SECONDS", "15")))
    parser.add_argument("--health-timeout-seconds", type=int, default=int(env("AW_RUS_SLO_HEALTH_TIMEOUT_SECONDS", "90")))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    aw_base = args.aw_base.rstrip("/")
    worktime_base = args.worktime_base.rstrip("/")
    args.aw_webui_url = f"{aw_base}/"
    args.worktime_today_html_url = f"{worktime_base}/reports/worktime/today?format=html&day=today&host={args.host}&allow_stale=1"
    args.worktime_management_html_url = f"{worktime_base}/reports/worktime/management?format=html&day=today&host={args.host}&allow_stale=1"
    args.worktime_today_csv_url = f"{worktime_base}/reports/worktime/today?format=csv&day=today&host={args.host}&allow_stale=1"
    args.worktime_management_json_url = f"{worktime_base}/reports/worktime/management?format=json&day=today&host={args.host}&allow_stale=1"

    state_dir = Path(args.state_dir)
    sample_path = state_dir / "aw-slo-samples.jsonl"
    sample = build_sample(args)
    samples = append_and_trim_sample(
        sample_path,
        sample,
        retention_seconds=max(1, args.retention_days) * 86400,
    )

    generated_at = now_utc()
    summary = {
        "generated_at_utc": iso(generated_at),
        "target_percent": args.target_percent,
        "sample_interval_seconds": args.sample_interval_seconds,
        "current_sample": sample,
        "windows": {
            "24h": summarize_window(
                samples,
                now=generated_at,
                window_seconds=86400,
                sample_interval_seconds=args.sample_interval_seconds,
                target_percent=args.target_percent,
            ),
            "7d": summarize_window(
                samples,
                now=generated_at,
                window_seconds=7 * 86400,
                sample_interval_seconds=args.sample_interval_seconds,
                target_percent=args.target_percent,
            ),
            "30d": summarize_window(
                samples,
                now=generated_at,
                window_seconds=30 * 86400,
                sample_interval_seconds=args.sample_interval_seconds,
                target_percent=args.target_percent,
            ),
        },
    }
    summary_json_path = state_dir / "aw-slo-summary.json"
    summary_txt_path = state_dir / "aw-slo-summary.txt"
    write_atomic(summary_json_path, json.dumps(summary, ensure_ascii=False, indent=2) + "\n")
    text = render_summary_text(summary)
    write_atomic(summary_txt_path, text + "\n")
    chmod_if_possible(summary_json_path, 0o644)
    chmod_if_possible(summary_txt_path, 0o644)
    chown_like_parent_if_possible(summary_json_path)
    chown_like_parent_if_possible(summary_txt_path)
    if args.json:
        print(json.dumps(summary, ensure_ascii=False, indent=2))
    else:
        print(text)
    return 0 if sample["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
