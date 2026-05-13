#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from dictionary_matcher import match_text
from ocr_processor import analyze_screenshot

BASE_DIR = Path(__file__).resolve().parent
DICTIONARY_DIR = BASE_DIR / "dictionaries"
REGEX_PACK_DIR = BASE_DIR / "regex-packs"


def resolve_dictionary_pack(name: str | None) -> str | None:
    if not name:
        return None
    path = DICTIONARY_DIR / f"{name}.json"
    return str(path) if path.exists() else None


def resolve_regex_pack(name: str | None) -> str | None:
    if not name:
        return None
    path = REGEX_PACK_DIR / f"{name}.json"
    return str(path) if path.exists() else None


def analyze_text_content(
    text: str,
    dictionary_pack: str | None = None,
    regex_pack: str | None = None,
) -> dict[str, Any]:
    dictionary_path = resolve_dictionary_pack(dictionary_pack)
    regex_pack_path = resolve_regex_pack(regex_pack)
    matches = match_text(
        text=text,
        dictionary_path=dictionary_path,
        regex_pack_path=regex_pack_path,
    )
    return {
        "text": text,
        "dictionary_pack": dictionary_pack,
        "regex_pack": regex_pack,
        "dictionary_matches": matches.get("dictionary_matches", []),
        "regex_matches": matches.get("regex_matches", []),
    }


def analyze_artifact(
    text: str | None = None,
    image_path: str | None = None,
    dictionary_pack: str | None = None,
    regex_pack: str | None = None,
) -> dict[str, Any]:
    if image_path:
        dictionary_path = resolve_dictionary_pack(dictionary_pack)
        regex_pack_path = resolve_regex_pack(regex_pack)
        result = analyze_screenshot(
            image_path=image_path,
            dictionary_path=dictionary_path,
            regex_pack_path=regex_pack_path,
        )
        result["dictionary_pack"] = dictionary_pack
        result["regex_pack"] = regex_pack
        result["source"] = "image"
        return result
    return {
        **analyze_text_content(
            text=text or "",
            dictionary_pack=dictionary_pack,
            regex_pack=regex_pack,
        ),
        "source": "text",
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Analyze text or screenshot with DLP dictionaries/regex packs")
    parser.add_argument("--text", help="Text to analyze")
    parser.add_argument("--image", help="Screenshot/image path to analyze")
    parser.add_argument("--dictionary-pack", default=None)
    parser.add_argument("--regex-pack", default=None)
    args = parser.parse_args()

    result = analyze_artifact(
        text=args.text,
        image_path=args.image,
        dictionary_pack=args.dictionary_pack,
        regex_pack=args.regex_pack,
    )
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
