# DLP Policy Engine

## Purpose

`aw-server/dlp-policy-engine` centralizes DLP policy lifecycle for `AWatch-rus` Windows endpoints.

It does not replace endpoint-local safety. Endpoints can run in:
- `local`
- `server`
- `cached` fallback after server outage

## API

Base URL:

```text
http://<aw-server>:5601
```

Routes:

- `GET /healthz`
- `GET /api/0/dlp/policies`
- `POST /api/0/dlp/policies`
- `GET /api/0/dlp/policies/active`
- `POST /api/0/dlp/policies/rollback`
- `GET /api/0/dlp/policies/{id}`
- `PUT /api/0/dlp/policies/{id}`
- `POST /api/0/dlp/policies/{id}/activate`
- `DELETE /api/0/dlp/policies/{id}`

## Policy create example

```json
{
  "name": "base-windows-policy",
  "description": "Primary DLP policy for pilot endpoints",
  "activate": true,
  "actor": "ansible",
  "policy": {
    "version": 1,
    "defaults": {
      "enabled": true,
      "cooldownSeconds": 300,
      "action": "alert",
      "severity": "medium"
    },
    "endpoint": {
      "clipboard": [],
      "usb": [],
      "print": []
    }
  }
}
```

## Active policy response

```json
{
  "active": true,
  "policyId": 1,
  "name": "base-windows-policy",
  "version": 3,
  "checksum": "sha256...",
  "updatedAtUtc": "2026-05-11T12:00:00Z",
  "policy": {
    "version": 1,
    "defaults": {
      "enabled": true,
      "cooldownSeconds": 300,
      "action": "alert",
      "severity": "medium"
    },
    "endpoint": {
      "clipboard": [],
      "usb": [],
      "print": []
    }
  }
}
```

## Deployment Notes

- Service runs as `aw-dlp-policy-engine.service`.
- SQLite path is controlled by `AW_DLP_POLICY_ENGINE_DB_PATH`.
- Default port is `5601`.
- Endpoints should use `server` mode only after `GET /healthz` and `GET /api/0/dlp/policies/active` are confirmed.
