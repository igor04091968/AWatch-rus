#!/usr/bin/env python3
from __future__ import annotations

from datetime import datetime
from pathlib import Path


def render_html(period: str) -> str:
    return f"""<html><body><h1>Отчет 152-ФЗ</h1><p>Период: {period}</p><p>Сгенерирован: {datetime.now().isoformat()}</p></body></html>"""


def main() -> None:
    period = datetime.now().strftime("%Y-%m")
    out = Path("/opt/activitywatch/dlp-compliance/reports")
    out.mkdir(parents=True, exist_ok=True)
    html = out / f"152-fz-{period}.html"
    html.write_text(render_html(period), encoding="utf-8")


if __name__ == "__main__":
    main()
