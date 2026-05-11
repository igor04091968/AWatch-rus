#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import re
from typing import Any

from checksum_validator import validate_inn, validate_snils


def _validate(kind: str, value: str) -> bool:
    if kind == "inn":
        return validate_inn(value)
    if kind == "snils":
        return validate_snils(value)
    return True


def match_text(text: str, dictionary_path: str) -> list[dict[str, Any]]:
    rules = json.loads(pathlib.Path(dictionary_path).read_text(encoding="utf-8"))
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
