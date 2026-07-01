#!/usr/bin/env python3
"""Validate DetMir production binary parity evidence.

The script compares SHA256 hashes collected from running production binaries
with locally built release artifacts from the same Git revision. It intentionally
does not collect live production data: operators provide evidence produced from
the approved production contour.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
WINDOWS_TARGET = "x86_64-pc-windows-gnu"
REQUIRED_FIELDS = (
    "id",
    "host",
    "kind",
    "unit_or_task",
    "binary_path",
    "crate",
    "release_artifact",
    "runtime_role",
    "production_sha256",
)


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_target_root(root: Path) -> Path:
    return Path(os.environ.get("CARGO_TARGET_DIR", root / "adk-rust" / "target"))


def default_release_dir(root: Path) -> Path:
    return default_target_root(root) / "release"


def default_windows_release_dir(root: Path) -> Path:
    return default_target_root(root) / WINDOWS_TARGET / "release"


def git_head(root: Path) -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=root,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return completed.stdout.strip().lower()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    if not isinstance(data, dict):
        raise ValueError("evidence root must be a JSON object")
    return data


def is_windows_item(item: dict[str, Any]) -> bool:
    platform = str(item.get("platform", "")).lower()
    artifact = str(item.get("release_artifact", "")).lower()
    binary_path = str(item.get("binary_path", ""))
    return platform == "windows" or artifact.endswith(".exe") or "\\" in binary_path


def artifact_path(
    item: dict[str, Any],
    release_dir: Path,
    windows_release_dir: Path,
) -> Path:
    artifact = str(item.get("release_artifact", ""))
    if not artifact or "/" in artifact or "\\" in artifact:
        raise ValueError("release_artifact must be a file name, not a path")
    base = windows_release_dir if is_windows_item(item) else release_dir
    return base / artifact


def normalize_sha(value: Any) -> str:
    return str(value).strip().lower()


def validate_evidence(
    evidence: dict[str, Any],
    *,
    expected_git_sha: str,
    release_dir: Path,
    windows_release_dir: Path,
) -> dict[str, Any]:
    errors: list[str] = []
    rows: list[dict[str, Any]] = []
    skipped: list[dict[str, Any]] = []

    schema_version = evidence.get("schema_version")
    if schema_version != 1:
        errors.append("schema_version must be 1")

    git_sha = normalize_sha(evidence.get("git_sha", ""))
    if not HEX40.match(git_sha):
        errors.append("top-level git_sha must be a 40-character lowercase hex SHA")
    elif git_sha != expected_git_sha:
        errors.append(
            f"top-level git_sha {git_sha} does not match expected {expected_git_sha}"
        )

    items = evidence.get("items")
    if not isinstance(items, list) or not items:
        errors.append("items must be a non-empty array")
        items = []

    seen_ids: set[str] = set()
    active_count = 0

    for index, raw_item in enumerate(items):
        if not isinstance(raw_item, dict):
            errors.append(f"items[{index}] must be an object")
            continue
        item = raw_item
        item_id = str(item.get("id", f"items[{index}]"))

        if item_id in seen_ids:
            errors.append(f"{item_id}: duplicate id")
        seen_ids.add(item_id)

        raw_active = item.get("active", True)
        if not isinstance(raw_active, bool):
            errors.append(f"{item_id}: active must be boolean")
            continue
        active = raw_active
        if not active:
            skip_reason = str(item.get("skip_reason", "")).strip()
            if not skip_reason:
                errors.append(f"{item_id}: inactive item must include skip_reason")
            skipped.append(
                {
                    "id": item_id,
                    "host": item.get("host", ""),
                    "unit_or_task": item.get("unit_or_task", ""),
                    "crate": item.get("crate", ""),
                    "runtime_role": item.get("runtime_role", ""),
                    "skip_reason": skip_reason,
                }
            )
            continue

        active_count += 1

        for field in REQUIRED_FIELDS:
            value = item.get(field)
            if value is None or str(value).strip() == "":
                errors.append(f"{item_id}: missing required field {field}")

        item_git_sha = item.get("git_sha")
        if item_git_sha is not None and normalize_sha(item_git_sha) != expected_git_sha:
            errors.append(
                f"{item_id}: item git_sha {normalize_sha(item_git_sha)} "
                f"does not match expected {expected_git_sha}"
            )

        production_sha = normalize_sha(item.get("production_sha256", ""))
        if not HEX64.match(production_sha):
            errors.append(f"{item_id}: production_sha256 must be 64 lowercase hex chars")
            continue

        try:
            local_artifact = artifact_path(item, release_dir, windows_release_dir)
        except ValueError as exc:
            errors.append(f"{item_id}: {exc}")
            continue

        if not local_artifact.is_file():
            errors.append(f"{item_id}: release artifact not found: {local_artifact}")
            continue

        release_sha = sha256_file(local_artifact)
        parity = release_sha == production_sha
        if not parity:
            errors.append(
                f"{item_id}: production SHA {production_sha} does not match "
                f"release SHA {release_sha} for {local_artifact}"
            )

        rows.append(
            {
                "id": item_id,
                "host": item.get("host", ""),
                "kind": item.get("kind", ""),
                "unit_or_task": item.get("unit_or_task", ""),
                "binary_path": item.get("binary_path", ""),
                "crate": item.get("crate", ""),
                "release_artifact": item.get("release_artifact", ""),
                "runtime_role": item.get("runtime_role", ""),
                "production_sha256": production_sha,
                "release_sha256": release_sha,
                "git_sha": expected_git_sha,
                "parity": parity,
            }
        )

    if active_count == 0:
        errors.append("at least one active production binary must be present")

    return {
        "status": "ok" if not errors else "fail",
        "schema_version": schema_version,
        "git_sha": expected_git_sha,
        "release_dir": str(release_dir),
        "windows_release_dir": str(windows_release_dir),
        "active_count": active_count,
        "skipped_count": len(skipped),
        "items": rows,
        "skipped": skipped,
        "errors": errors,
    }


def print_report(report: dict[str, Any]) -> None:
    if report["status"] == "ok":
        print(
            "production_binary_parity=ok "
            f"active={report['active_count']} skipped={report['skipped_count']} "
            f"git_sha={report['git_sha']}"
        )
        for item in report["items"]:
            print(
                "OK "
                f"{item['unit_or_task']} -> {item['binary_path']} -> "
                f"{item['crate']} role={item['runtime_role']} "
                f"sha256={item['production_sha256']}"
            )
        for item in report["skipped"]:
            print(
                "SKIP "
                f"{item['unit_or_task']} crate={item['crate']} "
                f"reason={item['skip_reason']}"
            )
        return

    print(
        "production_binary_parity=fail "
        f"active={report['active_count']} skipped={report['skipped_count']} "
        f"git_sha={report['git_sha']}",
        file=sys.stderr,
    )
    for error in report["errors"]:
        print(f"ERROR {error}", file=sys.stderr)


def write_json_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2, sort_keys=True)
        handle.write("\n")


def run_self_test() -> int:
    root = Path(tempfile.mkdtemp(prefix="aw-parity-selftest-"))
    try:
        release_dir = root / "release"
        windows_release_dir = root / WINDOWS_TARGET / "release"
        release_dir.mkdir(parents=True)
        windows_release_dir.mkdir(parents=True)

        linux_artifact = release_dir / "detmir-readiness"
        windows_artifact = windows_release_dir / "aw-windows-telemetry.exe"
        linux_artifact.write_bytes(b"linux-release-artifact\n")
        windows_artifact.write_bytes(b"windows-release-artifact\n")

        git_sha = "0123456789abcdef0123456789abcdef01234567"
        evidence = {
            "schema_version": 1,
            "generated_at_utc": "2026-07-01T00:00:00Z",
            "git_sha": git_sha,
            "items": [
                {
                    "id": "server:detmir-readiness.service",
                    "host": "10.10.10.13",
                    "kind": "systemd_service",
                    "unit_or_task": "detmir-readiness.service",
                    "binary_path": "/usr/local/bin/detmir-readiness-rust",
                    "crate": "detmir-readiness",
                    "release_artifact": "detmir-readiness",
                    "runtime_role": "readiness check",
                    "production_sha256": sha256_file(linux_artifact),
                    "active": True,
                },
                {
                    "id": "rdp:aw-windows-telemetry",
                    "host": "192.168.100.19",
                    "kind": "windows_scheduled_task",
                    "unit_or_task": "AWatch-rus telemetry collector",
                    "binary_path": (
                        r"C:\Program Files\AWatch-rus\windows"
                        r"\aw-windows-telemetry.exe"
                    ),
                    "crate": "aw-windows-telemetry",
                    "release_artifact": "aw-windows-telemetry.exe",
                    "runtime_role": "Windows telemetry collector",
                    "production_sha256": sha256_file(windows_artifact),
                    "platform": "windows",
                    "active": True,
                },
                {
                    "id": "optional:dlp-aggregator",
                    "host": "10.10.10.13",
                    "kind": "systemd_service",
                    "unit_or_task": "dlp-aggregator.service",
                    "binary_path": "/usr/local/bin/dlp-aggregator-rust",
                    "crate": "dlp-aggregator",
                    "release_artifact": "dlp-aggregator",
                    "runtime_role": "optional DLP aggregator",
                    "active": False,
                    "skip_reason": "DLP runtime intentionally disabled",
                },
            ],
        }

        ok_report = validate_evidence(
            evidence,
            expected_git_sha=git_sha,
            release_dir=release_dir,
            windows_release_dir=windows_release_dir,
        )
        if ok_report["status"] != "ok":
            print(json.dumps(ok_report, indent=2), file=sys.stderr)
            return 1

        mismatch = json.loads(json.dumps(evidence))
        mismatch["items"][0]["production_sha256"] = "0" * 64
        fail_report = validate_evidence(
            mismatch,
            expected_git_sha=git_sha,
            release_dir=release_dir,
            windows_release_dir=windows_release_dir,
        )
        if fail_report["status"] != "fail":
            print(json.dumps(fail_report, indent=2), file=sys.stderr)
            return 1

        print("check_production_binary_parity self-test: OK")
        return 0
    finally:
        shutil.rmtree(root)


def parse_args(argv: list[str]) -> argparse.Namespace:
    root = repo_root()
    parser = argparse.ArgumentParser(
        description="Validate DetMir production binary parity evidence."
    )
    parser.add_argument("--evidence", type=Path, help="production evidence JSON")
    parser.add_argument(
        "--release-dir",
        type=Path,
        default=default_release_dir(root),
        help="Linux release artifact directory",
    )
    parser.add_argument(
        "--windows-release-dir",
        type=Path,
        default=default_windows_release_dir(root),
        help="Windows release artifact directory",
    )
    parser.add_argument(
        "--git-sha",
        default=None,
        help="expected source Git SHA; defaults to repository HEAD",
    )
    parser.add_argument("--output-json", type=Path, help="write validation report")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run internal validator regression test",
    )
    args = parser.parse_args(argv)
    if not args.self_test and args.evidence is None:
        parser.error("--evidence is required unless --self-test is used")
    return args


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        return run_self_test()

    root = repo_root()
    expected_git_sha = normalize_sha(args.git_sha or git_head(root))
    if not HEX40.match(expected_git_sha):
        print(
            f"expected git SHA must be a 40-character lowercase hex SHA: {expected_git_sha}",
            file=sys.stderr,
        )
        return 2

    try:
        evidence = load_json(args.evidence)
        report = validate_evidence(
            evidence,
            expected_git_sha=expected_git_sha,
            release_dir=args.release_dir,
            windows_release_dir=args.windows_release_dir,
        )
    except (OSError, ValueError, json.JSONDecodeError, subprocess.CalledProcessError) as exc:
        print(f"production_binary_parity=fail error={exc}", file=sys.stderr)
        return 1

    if args.output_json:
        write_json_report(args.output_json, report)

    print_report(report)
    return 0 if report["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
