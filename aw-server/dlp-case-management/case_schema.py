#!/usr/bin/env python3
from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field

CaseStatus = Literal["open", "investigating", "resolved", "closed"]


class CaseHayabusaLink(BaseModel):
    tool: Literal["hayabusa"] = "hayabusa"
    host: str = Field(min_length=1, max_length=128)
    mode: str = Field(min_length=1, max_length=32)
    status: str = Field(min_length=1, max_length=64)
    intake_id: str | None = Field(default=None, max_length=256)
    package_path: str | None = Field(default=None, max_length=1024)
    sha256: str | None = Field(default=None, max_length=128)
    report_dir: str | None = Field(default=None, max_length=1024)
    summary_html: str | None = Field(default=None, max_length=1024)
    timeline_path: str | None = Field(default=None, max_length=1024)
    manifest_path: str | None = Field(default=None, max_length=1024)
    linked_at: str | None = Field(default=None, max_length=64)
    link_source: str | None = Field(default=None, max_length=64)


class CaseCreate(BaseModel):
    incident_id: str = Field(min_length=1, max_length=256)
    host: str | None = Field(default=None, max_length=128)
    title: str = Field(min_length=1, max_length=512)
    severity: str = Field(default="medium", max_length=32)
    assignee: str | None = Field(default=None, max_length=128)
    source_bucket: str | None = Field(default=None, max_length=256)
    source_event_ts: str | None = Field(default=None, max_length=64)
    evidence: dict | None = None


class CaseUpdate(BaseModel):
    status: CaseStatus | None = None
    assignee: str | None = Field(default=None, max_length=128)
    title: str | None = Field(default=None, max_length=512)
    severity: str | None = Field(default=None, max_length=32)


class CaseCommentCreate(BaseModel):
    comment: str = Field(min_length=1, max_length=2000)
    author: str | None = Field(default=None, max_length=128)


class CaseComment(BaseModel):
    id: int
    case_id: int
    comment: str
    author: str | None
    created_at: datetime


class CaseRecord(BaseModel):
    id: int
    incident_id: str
    host: str | None
    title: str
    severity: str
    assignee: str | None
    status: CaseStatus
    source_bucket: str | None
    source_event_ts: str | None
    evidence: dict | None
    forensics: dict | None
    created_at: datetime
    updated_at: datetime
