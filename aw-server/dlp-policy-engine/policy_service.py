#!/usr/bin/env python3
from __future__ import annotations

import os
from pathlib import Path

from fastapi import FastAPI, HTTPException

from policy_distributor import build_policy_bundle
from policy_schema import PolicyActivateRequest, PolicyCreateRequest, PolicyUpdateRequest
from policy_storage import PolicyStorage


def _env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


APP_NAME = "aw-dlp-policy-engine"
APP_VERSION = "0.1.0"
DB_PATH = _env("AW_DLP_POLICY_ENGINE_DB_PATH", "/var/lib/activitywatch/dlp-policy-engine.sqlite")
storage = PolicyStorage(DB_PATH)

app = FastAPI(title=APP_NAME, version=APP_VERSION)


@app.get("/healthz")
def healthz() -> dict[str, str]:
    return {
        "status": "ok",
        "service": APP_NAME,
        "db_path": DB_PATH,
        "db_exists": str(Path(DB_PATH).exists()).lower(),
    }


@app.get("/api/0/dlp/policies")
def list_policies() -> dict[str, object]:
    return {"items": storage.list_policies()}


@app.post("/api/0/dlp/policies", status_code=201)
def create_policy(payload: PolicyCreateRequest) -> dict[str, object]:
    try:
        item = storage.create_policy(
            name=payload.name,
            description=payload.description,
            policy=payload.policy.model_dump(mode="json"),
            activate=payload.activate,
            actor=payload.actor,
        )
    except Exception as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return {"item": item}


@app.get("/api/0/dlp/policies/active")
def get_active_policy() -> dict[str, object]:
    item = storage.get_active_policy()
    if not item:
        raise HTTPException(status_code=404, detail="no active policy configured")
    return build_policy_bundle(item)


@app.post("/api/0/dlp/policies/rollback")
def rollback_active_policy(payload: PolicyActivateRequest) -> dict[str, object]:
    item = storage.rollback_active_policy(actor=payload.actor)
    if not item:
        raise HTTPException(status_code=404, detail="no active policy configured")
    return {"item": item}


@app.get("/api/0/dlp/policies/{policy_id}")
def get_policy(policy_id: int) -> dict[str, object]:
    item = storage.get_policy(policy_id)
    if not item:
        raise HTTPException(status_code=404, detail="policy not found")
    return {"item": item}


@app.put("/api/0/dlp/policies/{policy_id}")
def update_policy(policy_id: int, payload: PolicyUpdateRequest) -> dict[str, object]:
    try:
        item = storage.update_policy(
            policy_id=policy_id,
            name=payload.name,
            description=payload.description,
            policy=payload.policy.model_dump(mode="json") if payload.policy is not None else None,
            activate=payload.activate,
            actor=payload.actor,
        )
    except Exception as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    if not item:
        raise HTTPException(status_code=404, detail="policy not found")
    return {"item": item}


@app.post("/api/0/dlp/policies/{policy_id}/activate")
def activate_policy(policy_id: int, payload: PolicyActivateRequest) -> dict[str, object]:
    item = storage.activate_policy(policy_id=policy_id, actor=payload.actor)
    if not item:
        raise HTTPException(status_code=404, detail="policy not found")
    return {"item": item}


@app.delete("/api/0/dlp/policies/{policy_id}")
def delete_policy(policy_id: int) -> dict[str, bool]:
    try:
        deleted = storage.delete_policy(policy_id)
    except ValueError as exc:
        raise HTTPException(status_code=409, detail=str(exc)) from exc

    if not deleted:
        raise HTTPException(status_code=404, detail="policy not found")
    return {"deleted": True}
