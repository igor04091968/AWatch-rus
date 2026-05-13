#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

from PIL import Image
import pytesseract

from dictionary_matcher import match_text


def extract_text(image_path: str) -> str:
    path = Path(image_path)
    if not path.exists():
        return ""
    img = Image.open(path)
    return pytesseract.image_to_string(img, lang="rus+eng")


def analyze_screenshot(
    image_path: str,
    dictionary_path: str | None = None,
    regex_pack_path: str | None = None,
) -> dict:
    text = extract_text(image_path)
    if not text:
        return {"text": "", "dictionary_matches": [], "regex_matches": []}
    result = match_text(text=text, dictionary_path=dictionary_path, regex_pack_path=regex_pack_path)
    result["text"] = text
    return result
