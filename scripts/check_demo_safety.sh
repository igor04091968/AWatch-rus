#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - "$@" <<'PY'
import ipaddress
import json
import re
import sys
from pathlib import Path

ROOT = Path.cwd()

JSON_MODE = "--json" in sys.argv[1:]

TEXT_SUFFIXES = {
    ".csv",
    ".html",
    ".htm",
    ".json",
    ".md",
    ".mjs",
    ".sql",
    ".txt",
    ".yaml",
    ".yml",
}

SAFE_IP_NETWORKS = [
    ipaddress.ip_network("192.0.2.0/24"),
    ipaddress.ip_network("198.51.100.0/24"),
    ipaddress.ip_network("203.0.113.0/24"),
]

SAFE_IPS = {
    ipaddress.ip_address("0.0.0.0"),
    ipaddress.ip_address("127.0.0.1"),
}

SAFE_EMAIL_DOMAINS = {
    "example.com",
    "example.net",
    "example.org",
    "invalid",
    "localhost",
}

SAFE_SECRET_VALUES = {
    "",
    "***",
    "****",
    "xxxxx",
    "xxxx",
    "change_me",
    "changeme",
    "redacted",
    "placeholder",
    "example",
    "demo",
    "demo-only",
    "<token>",
    "<secret>",
    "<password>",
    "<cookie>",
}

IPV4_RE = re.compile(r"(?<![\d.])(?:\d{1,3}\.){3}\d{1,3}(?![\d.])")
EMAIL_RE = re.compile(r"\b[A-Za-z0-9._%+-]+@([A-Za-z0-9.-]+\.[A-Za-z]{2,}|localhost|invalid)\b")
FORBIDDEN_DOMAIN_RE = re.compile(
    r"\b(?:dm\.iri|sevnb\.ru|msk\.sevnb\.ru|dns\.sevnb\.ru|iri1968|SHARKON2025)\b",
    re.IGNORECASE,
)
SECRET_ASSIGN_RE = re.compile(
    r"""(?ix)
    \b(password|passwd|pwd|token|secret|api[_-]?key|bearer|cookie|session[_-]?(?:id|secret|token)?)\b
    \s*[:=]\s*
    ["']?([^"'\s,;}]+)
    """
)
BEARER_RE = re.compile(r"\bAuthorization\s*:\s*Bearer\s+([A-Za-z0-9._~+/=-]+)", re.IGNORECASE)
WORKSTATION_RE = re.compile(r"\b(?:DESKTOP|LAPTOP|WIN|WS)-[A-Z0-9][A-Z0-9-]{3,}\b")
CYRILLIC_FIO_RE = re.compile(r"\b[А-ЯЁ][а-яё]{2,}\s+[А-ЯЁ][а-яё]{2,}\s+[А-ЯЁ][а-яё]{2,}\b")


def is_safe_ip(value: str) -> bool:
    try:
        ip = ipaddress.ip_address(value)
    except ValueError:
        return True
    return ip in SAFE_IPS or any(ip in network for network in SAFE_IP_NETWORKS)


def is_safe_email(domain: str, email: str) -> bool:
    normalized = domain.lower()
    if normalized in SAFE_EMAIL_DOMAINS:
        return True
    if normalized.endswith(".example") or normalized.endswith(".invalid"):
        return True
    return "demo" in email.lower()


def is_safe_secret_value(value: str) -> bool:
    cleaned = value.strip().strip("\"'").strip()
    lowered = cleaned.lower()
    if lowered in SAFE_SECRET_VALUES:
        return True
    if cleaned.startswith("<") and cleaned.endswith(">"):
        return True
    if set(cleaned) <= {"*", "x", "X", "-"}:
        return True
    return False


def collect_scope() -> list[Path]:
    files = [ROOT / "README.md"]
    for path in (ROOT / "docs").rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(ROOT).as_posix()
        name = path.name
        if rel.startswith(("docs/demo/", "docs/fixtures/", "docs/screenshots/", "docs/assets/screenshots/")):
            files.append(path)
            continue
        if any(marker in name for marker in ("DEMO", "PILOT", "CUSTOMER", "RELEASE_READINESS", "FREEZE")):
            files.append(path)
    return sorted(set(files))


def add_finding(findings: list[dict], path: Path, line_no: int, rule: str, value: str) -> None:
    findings.append(
        {
            "file": path.relative_to(ROOT).as_posix(),
            "line": line_no,
            "rule": rule,
            "value": value,
        }
    )


def is_allowed_registry_infrastructure_reference(path: Path, line: str, value: str) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    if rel != "README.md":
        return False
    return value.lower() == "iri1968" and "https://git.iri1968.dpdns.org" in line


def scan_line(findings: list[dict], path: Path, line_no: int, line: str) -> None:
    stripped = line.strip()
    if stripped.startswith("#") and "grep -n" in stripped:
        return

    for match in IPV4_RE.finditer(line):
        value = match.group(0)
        if not is_safe_ip(value):
            add_finding(findings, path, line_no, "real_ipv4_not_in_demo_range", value)

    for match in EMAIL_RE.finditer(line):
        email = match.group(0)
        domain = match.group(1)
        if not is_safe_email(domain, email):
            add_finding(findings, path, line_no, "non_demo_email", email)

    for match in FORBIDDEN_DOMAIN_RE.finditer(line):
        value = match.group(0)
        if not is_allowed_registry_infrastructure_reference(path, line, value):
            add_finding(findings, path, line_no, "known_live_domain_or_codename", value)

    for match in SECRET_ASSIGN_RE.finditer(line):
        value = match.group(2)
        if not is_safe_secret_value(value):
            add_finding(findings, path, line_no, "credential_like_assignment", match.group(0))

    for match in BEARER_RE.finditer(line):
        value = match.group(1)
        if not is_safe_secret_value(value):
            add_finding(findings, path, line_no, "bearer_secret", "Authorization: Bearer ...")

    for match in WORKSTATION_RE.finditer(line):
        value = match.group(0)
        if "DEMO" not in value:
            add_finding(findings, path, line_no, "realistic_workstation_name", value)

    for match in CYRILLIC_FIO_RE.finditer(line):
        add_finding(findings, path, line_no, "possible_cyrillic_fio", match.group(0))


def scan_text_file(path: Path) -> list[dict]:
    findings: list[dict] = []
    text = path.read_text(encoding="utf-8", errors="ignore")
    for line_no, line in enumerate(text.splitlines(), start=1):
        scan_line(findings, path, line_no, line)
    return findings


def scan_png(path: Path) -> list[dict]:
    findings: list[dict] = []
    data = path.read_bytes()
    png_signature = b"\x89PNG\r\n\x1a\n"
    if len(data) <= 1000 or not data.startswith(png_signature):
        add_finding(findings, path, 0, "screenshot_not_png_or_too_small", str(len(data)))
        return findings
    metadata_text = data.decode("latin-1", errors="ignore")
    for line_no, line in enumerate(metadata_text.splitlines(), start=1):
        scan_line(findings, path, line_no, line)
    return findings


def main() -> int:
    files = collect_scope()
    findings: list[dict] = []

    for path in files:
        if path.suffix.lower() == ".png":
            findings.extend(scan_png(path))
        elif path.suffix.lower() in TEXT_SUFFIXES:
            findings.extend(scan_text_file(path))

    result = {
        "ok": not findings,
        "scope_files": [path.relative_to(ROOT).as_posix() for path in files],
        "findings": findings,
    }

    if JSON_MODE:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    elif findings:
        print("Demo safety check failed:", file=sys.stderr)
        for finding in findings:
            print(
                f"{finding['file']}:{finding['line']}: {finding['rule']}: {finding['value']}",
                file=sys.stderr,
            )
    else:
        print(f"Demo safety check passed ({len(files)} files scanned).")

    return 0 if not findings else 2


if __name__ == "__main__":
    raise SystemExit(main())
PY
