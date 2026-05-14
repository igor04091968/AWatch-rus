#!/usr/bin/env python3
from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException, Query
from fastapi.middleware.cors import CORSMiddleware

from case_schema import CaseCommentCreate, CaseCreate, CaseHayabusaLink, CaseUpdate
from case_storage import CaseStorage

DB = Path(os.environ.get("AW_DLP_CASE_DB_PATH", "/opt/activitywatch/dlp-case-management/cases.db"))
APP = FastAPI(title="AWatch DLP Case Management")
APP.add_middleware(
    CORSMiddleware,
    allow_origins=["http://127.0.0.1:5600", "http://localhost:5600", "http://10.10.10.13:5600", "*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)
STORE = CaseStorage(DB)


@APP.get("/health")
def health() -> dict[str, Any]:
    return {"ok": True, "db": str(DB)}


@APP.post("/api/0/dlp/cases")
def create_case(payload: CaseCreate) -> dict[str, Any]:
    return STORE.create_case(payload.model_dump(exclude_none=True), actor="api")


@APP.get("/api/0/dlp/cases")
def list_cases(
    status: str | None = Query(default=None),
    host: str | None = Query(default=None),
    limit: int = Query(default=200, ge=1, le=2000),
) -> list[dict[str, Any]]:
    return STORE.list_cases(status=status, host=host, limit=limit)


@APP.get("/api/0/dlp/cases/{case_id}")
def get_case(case_id: int) -> dict[str, Any]:
    try:
        case = STORE.get_case(case_id)
    except KeyError:
        raise HTTPException(status_code=404, detail="case not found")
    case["comments"] = STORE.list_comments(case_id, limit=200)
    case["audit"] = STORE.list_audit(case_id, limit=200)
    return case


@APP.patch("/api/0/dlp/cases/{case_id}")
def update_case(case_id: int, payload: CaseUpdate) -> dict[str, Any]:
    patch = payload.model_dump(exclude_none=True)
    if not patch:
        return STORE.get_case(case_id)
    try:
        return STORE.update_case(case_id, patch=patch, actor="api")
    except KeyError:
        raise HTTPException(status_code=404, detail="case not found")


@APP.post("/api/0/dlp/cases/{case_id}/comments")
def add_comment(case_id: int, payload: CaseCommentCreate) -> dict[str, Any]:
    try:
        STORE.get_case(case_id)
    except KeyError:
        raise HTTPException(status_code=404, detail="case not found")
    return STORE.add_comment(case_id=case_id, comment=payload.comment, author=payload.author)


@APP.get("/api/0/dlp/cases/{case_id}/comments")
def list_comments(case_id: int, limit: int = Query(default=200, ge=1, le=2000)) -> list[dict[str, Any]]:
    return STORE.list_comments(case_id=case_id, limit=limit)


@APP.post("/api/0/dlp/cases/{case_id}/forensics/hayabusa")
def link_hayabusa(case_id: int, payload: CaseHayabusaLink) -> dict[str, Any]:
    try:
        return STORE.link_hayabusa(case_id=case_id, payload=payload.model_dump(exclude_none=True), actor="api")
    except KeyError:
        raise HTTPException(status_code=404, detail="case not found")
