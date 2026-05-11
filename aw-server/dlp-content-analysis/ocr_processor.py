#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

from PIL import Image
import pytesseract


def extract_text(image_path: str) -> str:
    path = Path(image_path)
    if not path.exists():
        return ""
    img = Image.open(path)
    return pytesseract.image_to_string(img, lang="rus+eng")
