#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import re
import sys
from typing import Any

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from checksum_validator import validate_inn, validate_passport, validate_snils


def _validate(kind: str, value: str) -> bool:
    if kind == "inn":
        return validate_inn(value)
    if kind == "snils":
        return validate_snils(value)
    if kind == "passport":
        return validate_passport(value)
    return True


def _load_json(path: str) -> dict[str, Any]:
    return json.loads(pathlib.Path(path).read_text(encoding="utf-8"))


def match_text_with_dictionary(text: str, dictionary_path: str) -> list[dict[str, Any]]:
    rules = _load_json(dictionary_path)
    results: list[dict[str, Any]] = []
    for name, rule in rules.items():
        regex = re.compile(rule["regex"])
        checksum_kind = rule.get("checksum", "none")
        for m in regex.finditer(text):
            token = m.group(0)
            if _validate(checksum_kind, token):
                results.append(
                    {
                        "name": name,
                        "description": rule.get("description", name),
                        "value": token,
                        "start": m.start(),
                        "end": m.end(),
                    }
                )
    return results


def match_text_with_regex_pack(text: str, regex_pack_path: str) -> list[dict[str, Any]]:
    pack = _load_json(regex_pack_path)
    results: list[dict[str, Any]] = []
    entries: list[dict[str, Any]] = []
    if isinstance(pack.get("rules"), list):
        entries = [e for e in pack["rules"] if isinstance(e, dict)]
    elif isinstance(pack.get("patterns"), dict):
        entries = [{"id": k, **v} for k, v in pack["patterns"].items() if isinstance(v, dict)]

    for entry in entries:
        rule_id = entry.get("id") or entry.get("name") or "regex-rule"
        regex = re.compile(entry["regex"])
        for m in regex.finditer(text):
            results.append(
                {
                    "name": rule_id,
                    "description": entry.get("description", rule_id),
                    "value": m.group(0),
                    "start": m.start(),
                    "end": m.end(),
                    "severity": entry.get("severity", "medium"),
                }
            )
    return results


def match_text(
    text: str,
    dictionary_path: str | None = None,
    regex_pack_path: str | None = None,
) -> dict[str, list[dict[str, Any]]]:
    return {
        "dictionary_matches": match_text_with_dictionary(text, dictionary_path) if dictionary_path else [],
        "regex_matches": match_text_with_regex_pack(text, regex_pack_path) if regex_pack_path else [],
    }
