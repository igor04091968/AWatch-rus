#!/usr/bin/env python3
from __future__ import annotations

from fastapi import FastAPI
from fastapi.responses import PlainTextResponse

app = FastAPI(title="AWatch DLP Metrics")


@app.get("/metrics", response_class=PlainTextResponse)
def metrics() -> str:
    # Minimal exporter baseline for Prometheus scraping.
    return "\n".join(
        [
            "# HELP aw_dlp_exporter_up Exporter availability.",
            "# TYPE aw_dlp_exporter_up gauge",
            "aw_dlp_exporter_up 1",
        ]
    )
