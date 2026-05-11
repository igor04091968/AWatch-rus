#!/usr/bin/env python3
from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException

from policy_distributor import build_policy_bundle
from policy_schema import PolicyActivateRequest, PolicyCreateRequest, PolicyStatusRequest, PolicyUpdateRequest
from policy_storage import PolicyStorage


def _env(name: str, default: str) -> str:
    value = os.environ.get(name)
    return value if value not in (None, "") else default


APP_NAME = "aw-dlp-policy-engine"
APP_VERSION = "0.1.0"
DB_PATH = _env("AW_DLP_POLICY_ENGINE_DB_PATH", "/var/lib/activitywatch/dlp-policy-engine.sqlite")
storage = PolicyStorage(DB_PATH)

app = FastAPI(title=APP_NAME, version=APP_VERSION)
AGENT_STATE: dict[str, dict[str, Any]] = {}


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


@app.get("/api/0/dlp/policies/active/version")
def get_active_policy_version() -> dict[str, object]:
    item = storage.get_active_policy()
    if not item:
        raise HTTPException(status_code=404, detail="no active policy configured")
    return {
        "active": True,
        "policyId": item["id"],
        "version": item["current_version"],
        "checksum": item["checksum"],
        "updatedAtUtc": item["updated_at"],
    }


@app.post("/api/0/dlp/policies/agents/{agent_id}/heartbeat")
def update_agent_policy_heartbeat(agent_id: str, payload: dict[str, Any]) -> dict[str, object]:
    AGENT_STATE[agent_id] = {
        "agentId": agent_id,
        "hostname": payload.get("hostname") or agent_id,
        "version": payload.get("version"),
        "checksum": payload.get("checksum"),
        "updatedAtUtc": payload.get("updatedAtUtc"),
    }
    return {"ok": True, "agent": AGENT_STATE[agent_id]}


@app.get("/api/0/dlp/policies/agents/{agent_id}/desired")
def get_agent_desired_policy(agent_id: str) -> dict[str, object]:
    item = storage.get_active_policy()
    if not item:
        raise HTTPException(status_code=404, detail="no active policy configured")

    current = AGENT_STATE.get(agent_id, {})
    current_version = current.get("version")
    current_checksum = current.get("checksum")
    desired_version = item["current_version"]
    desired_checksum = item["checksum"]
    refresh_now = (str(current_version) != str(desired_version)) or (str(current_checksum) != str(desired_checksum))

    return {
        "agentId": agent_id,
        "refreshNow": refresh_now,
        "reason": "mismatch" if refresh_now else "up-to-date",
        "current": {
            "version": current_version,
            "checksum": current_checksum,
        },
        "desired": {
            "policyId": item["id"],
            "version": desired_version,
            "checksum": desired_checksum,
            "updatedAtUtc": item["updated_at"],
        },
    }


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
    try:
        item = storage.activate_policy(policy_id=policy_id, actor=payload.actor)
    except Exception as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    if not item:
        raise HTTPException(status_code=404, detail="policy not found")
    return {"item": item}


@app.post("/api/0/dlp/policies/{policy_id}/submit")
def submit_policy_for_approval(policy_id: int, payload: PolicyStatusRequest) -> dict[str, object]:
    item = storage.set_policy_status(policy_id, "pending_approval", payload.actor, payload.comment)
    if not item:
        raise HTTPException(status_code=404, detail="policy not found")
    return {"item": item}


@app.post("/api/0/dlp/policies/{policy_id}/approve")
def approve_policy(policy_id: int, payload: PolicyStatusRequest) -> dict[str, object]:
    item = storage.set_policy_status(policy_id, "approved", payload.actor, payload.comment)
    if not item:
        raise HTTPException(status_code=404, detail="policy not found")
    return {"item": item}


@app.post("/api/0/dlp/policies/{policy_id}/draft")
def return_policy_to_draft(policy_id: int, payload: PolicyStatusRequest) -> dict[str, object]:
    item = storage.set_policy_status(policy_id, "draft", payload.actor, payload.comment)
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


@app.get("/api/0/dlp/policies/audit")
def list_policy_audit(limit: int = 200) -> dict[str, object]:
    return {"items": storage.list_audit(policy_id=None, limit=max(1, min(limit, 1000)))}


@app.get("/api/0/dlp/policies/{policy_id}/audit")
def list_single_policy_audit(policy_id: int, limit: int = 200) -> dict[str, object]:
    if not storage.get_policy(policy_id):
        raise HTTPException(status_code=404, detail="policy not found")
    return {"items": storage.list_audit(policy_id=policy_id, limit=max(1, min(limit, 1000)))}
