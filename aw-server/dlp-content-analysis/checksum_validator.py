#!/usr/bin/env python3
from __future__ import annotations

import re


def validate_inn(value: str) -> bool:
    digits = re.sub(r"\D", "", value)
    if len(digits) == 10:
        coef = [2, 4, 10, 3, 5, 9, 4, 6, 8]
        chk = sum(int(digits[i]) * coef[i] for i in range(9)) % 11 % 10
        return chk == int(digits[9])
    if len(digits) == 12:
        c11 = [7, 2, 4, 10, 3, 5, 9, 4, 6, 8]
        c12 = [3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8]
        chk11 = sum(int(digits[i]) * c11[i] for i in range(10)) % 11 % 10
        chk12 = sum(int(digits[i]) * c12[i] for i in range(11)) % 11 % 10
        return chk11 == int(digits[10]) and chk12 == int(digits[11])
    return False


def validate_snils(value: str) -> bool:
    digits = re.sub(r"\D", "", value)
    if len(digits) != 11:
        return False
    number = digits[:9]
    checksum = int(digits[9:])
    s = sum(int(number[i]) * (9 - i) for i in range(9))
    if s < 100:
        expected = s
    elif s in (100, 101):
        expected = 0
    else:
        expected = s % 101
        if expected == 100:
            expected = 0
    return checksum == expected


def validate_passport(value: str) -> bool:
    """
    Lightweight Russian passport validator:
    - expects 10 digits (series+number), optionally with spaces
    - rejects obvious invalid placeholders (all same digit, all zeros)
    """
    digits = re.sub(r"\D", "", value)
    if len(digits) != 10:
        return False
    if digits == "0000000000":
        return False
    if len(set(digits)) == 1:
        return False
    return True
