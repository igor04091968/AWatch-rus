#!/usr/bin/env python3
from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Any

from fastapi import FastAPI
from pydantic import BaseModel

DB = Path("/opt/activitywatch/dlp-case-management/cases.db")
APP = FastAPI(title="AWatch DLP Case Management")


class CaseCreate(BaseModel):
    incident_id: str
    title: str
    severity: str = "medium"
    assignee: str | None = None


def _conn() -> sqlite3.Connection:
    DB.parent.mkdir(parents=True, exist_ok=True)
    c = sqlite3.connect(DB)
    c.execute(
        "CREATE TABLE IF NOT EXISTS cases (id INTEGER PRIMARY KEY, incident_id TEXT, title TEXT, severity TEXT, assignee TEXT, status TEXT DEFAULT 'open')"
    )
    return c


@APP.post("/api/0/dlp/cases")
def create_case(payload: CaseCreate) -> dict[str, Any]:
    c = _conn()
    cur = c.cursor()
    cur.execute(
        "INSERT INTO cases (incident_id,title,severity,assignee,status) VALUES (?,?,?,?,?)",
        (payload.incident_id, payload.title, payload.severity, payload.assignee, "open"),
    )
    c.commit()
    case_id = cur.lastrowid
    c.close()
    return {"id": case_id}


@APP.get("/api/0/dlp/cases")
def list_cases() -> list[dict[str, Any]]:
    c = _conn()
    rows = c.execute("SELECT id,incident_id,title,severity,assignee,status FROM cases ORDER BY id DESC").fetchall()
    c.close()
    return [
        {"id": r[0], "incident_id": r[1], "title": r[2], "severity": r[3], "assignee": r[4], "status": r[5]}
        for r in rows
    ]
