#!/usr/bin/env python3
"""Fail-closed public scan for obvious committed secrets.

The scanner intentionally prints only file, line and rule names. It never
prints the matched value.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

TEXT_SUFFIXES = {
    ".cfg",
    ".conf",
    ".env",
    ".ini",
    ".json",
    ".lock",
    ".md",
    ".py",
    ".rs",
    ".sh",
    ".toml",
    ".ts",
    ".txt",
    ".yaml",
    ".yml",
}

SKIP_DIRS = {
    ".git",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "bin",
    "dist",
    "node_modules",
    "release-evidence",
    "target",
}

SKIP_FILES = {
    "scripts/public_secret_pattern_check.py",
}

ALLOW_MARKERS = (
    "# public-secret-scan: allow dummy",
    "// public-secret-scan: allow dummy",
)

SAFE_LITERAL_VALUES = {
    "",
    "admin",
    "change_me",
    "change-me",
    "changeme",
    "dummy",
    "example",
    "placeholder",
    "redacted",
    "secret",
    "test",
    "<redacted>",
    "<set_via_env>",
    "<set-via-env>",
}

UNQUOTED_ASSIGNMENT_SUFFIXES = {
    ".cfg",
    ".conf",
    ".env",
    ".ini",
    ".sh",
    ".toml",
    ".yaml",
    ".yml",
}

SECRET_KEY_RE = re.compile(
    r"(?i)\b(password|passwd|pwd|token|secret|api[_-]?key|bearer|cookie|private[_-]?key)\b"
)
TOKEN_LITERAL_RE = re.compile(r"^[A-Za-z0-9_./+=:-]{8,}$")

QUOTED_ASSIGNMENT_RE = re.compile(
    r"(?ix)"
    r"\b(?P<key>[A-Z0-9_./-]*(?:password|passwd|pwd|token|secret|api[_-]?key|bearer|cookie|private[_-]?key)[A-Z0-9_./-]*)\b"
    r"\s*(?:[:=]|=>)\s*"
    r"(?P<prefix>r|br|rb|R|BR|RB)?"
    r"(?P<quote>['\"])(?P<value>[^'\"]{8,})(?P=quote)"
)

ENV_ASSIGNMENT_RE = re.compile(
    r"(?i)^\s*(?:export\s+)?"
    r"(?P<key>[A-Z0-9_./-]*(?:PASSWORD|PASSWD|PWD|TOKEN|SECRET|API[_-]?KEY|BEARER|COOKIE|PRIVATE[_-]?KEY)[A-Z0-9_./-]*)"
    r"\s*=\s*(?P<value>[A-Za-z0-9_./+=:-]{8,})"
    r"(?=\s*(?:#|$))"
)

PRIVATE_KEY_HEADER_RE = re.compile(
    r"-----BEGIN (?:RSA |OPENSSH |EC |DSA )?PRIVATE KEY-----"
)
AWS_ACCESS_KEY_RE = re.compile(r"\bAKIA[0-9A-Z]{16}\b")
GITHUB_TOKEN_RE = re.compile(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{30,}\b")
GITHUB_FINE_GRAINED_TOKEN_RE = re.compile(r"\bgithub_pat_[A-Za-z0-9_]{40,}\b")
SLACK_TOKEN_RE = re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{20,}\b")
GOOGLE_API_KEY_RE = re.compile(r"\bAIza[0-9A-Za-z_-]{35}\b")


def is_allowlisted(line: str) -> bool:
    return any(marker in line for marker in ALLOW_MARKERS)


def is_safe_literal(value: str) -> bool:
    normalized = value.strip().strip("'\"").lower()
    if normalized in SAFE_LITERAL_VALUES:
        return True
    if normalized.startswith(("{{", "{%")):
        return True
    if "{{" in normalized and "}}" in normalized:
        return True
    if normalized.startswith("<") and normalized.endswith(">"):
        return True
    if normalized.startswith(("env:", "env.", "process.env.", "${", "$")):
        return True
    if normalized.startswith(("c:\\", "/", "./", "../")):
        return True
    if "set_via_env" in normalized or "redacted" in normalized:
        return True
    return False


def quoted_assignment_findings(line: str) -> list[str]:
    if "re.compile" in line:
        return []

    findings: list[str] = []
    for match in QUOTED_ASSIGNMENT_RE.finditer(line):
        value = match.group("value")
        if not TOKEN_LITERAL_RE.match(value):
            continue
        if is_safe_literal(value):
            continue
        findings.append("secret_assignment")
    return findings


def env_assignment_finding(line: str) -> str | None:
    match = ENV_ASSIGNMENT_RE.search(line)
    if not match:
        return None
    value = match.group("value")
    if is_safe_literal(value):
        return None
    return "secret_assignment"


def scan_line(line: str, relative: Path) -> list[str]:
    if is_allowlisted(line):
        return []

    findings: list[str] = []
    if PRIVATE_KEY_HEADER_RE.search(line):
        findings.append("private_key_header")
    if AWS_ACCESS_KEY_RE.search(line):
        findings.append("aws_access_key")
    if GITHUB_TOKEN_RE.search(line) or GITHUB_FINE_GRAINED_TOKEN_RE.search(line):
        findings.append("github_token")
    if SLACK_TOKEN_RE.search(line):
        findings.append("slack_token")
    if GOOGLE_API_KEY_RE.search(line):
        findings.append("google_api_key")

    findings.extend(quoted_assignment_findings(line))

    if relative.suffix.lower() in UNQUOTED_ASSIGNMENT_SUFFIXES:
        env_finding = env_assignment_finding(line)
        if env_finding:
            findings.append(env_finding)

    return sorted(set(findings))


def iter_text_files(root: Path):
    candidates: list[Path]
    try:
        proc = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        candidates = [
            root / item.decode("utf-8", errors="ignore")
            for item in proc.stdout.split(b"\0")
            if item
        ]
    except (OSError, subprocess.CalledProcessError):
        candidates = sorted(root.rglob("*"))

    for path in candidates:
        if not path.is_file():
            continue
        relative = path.relative_to(root)
        relative_text = relative.as_posix()
        if any(part in SKIP_DIRS for part in relative.parts):
            continue
        if relative_text in SKIP_FILES:
            continue
        if path.suffix.lower() not in TEXT_SUFFIXES:
            continue
        yield path, relative


def main() -> int:
    findings: list[str] = []

    for path, relative in iter_text_files(ROOT):
        try:
            lines = path.read_text(encoding="utf-8", errors="ignore").splitlines()
        except OSError:
            continue

        for line_no, line in enumerate(lines, start=1):
            for rule in scan_line(line, relative):
                findings.append(f"{relative}:{line_no}:{rule}")

    if findings:
        print("secret_pattern_check=fail")
        for finding in findings:
            print(finding)
        return 2

    print("secret_pattern_check=ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
