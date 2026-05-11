from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, Field, ConfigDict


class PolicyDocument(BaseModel):
    model_config = ConfigDict(extra="allow")

    version: int = 1
    defaults: dict[str, Any] = Field(
        default_factory=lambda: {
            "enabled": True,
            "cooldownSeconds": 300,
            "action": "alert",
            "severity": "medium",
        }
    )
    endpoint: dict[str, list[dict[str, Any]]] = Field(
        default_factory=lambda: {
            "clipboard": [],
            "usb": [],
            "print": [],
        }
    )


class PolicyCreateRequest(BaseModel):
    name: str = Field(min_length=1, max_length=128)
    description: str | None = Field(default=None, max_length=2048)
    policy: PolicyDocument
    activate: bool = False
    actor: str | None = Field(default="api")


class PolicyUpdateRequest(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=128)
    description: str | None = Field(default=None, max_length=2048)
    policy: PolicyDocument | None = None
    activate: bool = False
    actor: str | None = Field(default="api")


class PolicyActivateRequest(BaseModel):
    actor: str | None = Field(default="api")


class PolicyRecord(BaseModel):
    id: int
    name: str
    description: str | None
    is_active: bool
    current_version: int
    checksum: str
    created_at: datetime
    updated_at: datetime


class PolicyVersionRecord(BaseModel):
    policy_id: int
    version: int
    checksum: str
    created_at: datetime
    created_by: str | None

