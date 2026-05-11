# DLP Policy Engine

## Purpose

`aw-server/dlp-policy-engine` is the centralized policy lifecycle service for `AWatch-rus` endpoints.

It supports:
- full CRUD for policy documents;
- policy versioning in SQLite;
- approval workflow (`draft -> pending_approval -> approved -> deployed`);
- audit trail for every policy mutation;
- agent push/pull coordination (`heartbeat` + `desired` refresh hint).

## Base URL

```text
http://<aw-server>:5601
```

## REST API

Health:
- `GET /healthz`

CRUD:
- `GET /api/0/dlp/policies`
- `POST /api/0/dlp/policies`
- `GET /api/0/dlp/policies/{id}`
- `PUT /api/0/dlp/policies/{id}`
- `DELETE /api/0/dlp/policies/{id}`

Active policy:
- `GET /api/0/dlp/policies/active`
- `GET /api/0/dlp/policies/active/version`
- `POST /api/0/dlp/policies/rollback`

Approval workflow:
- `POST /api/0/dlp/policies/{id}/submit` -> `pending_approval`
- `POST /api/0/dlp/policies/{id}/approve` -> `approved`
- `POST /api/0/dlp/policies/{id}/draft` -> `draft`
- `POST /api/0/dlp/policies/{id}/activate` -> deploy (allowed only from `approved`)

Audit:
- `GET /api/0/dlp/policies/audit?limit=200`
- `GET /api/0/dlp/policies/{id}/audit?limit=200`

Agent push/pull sync:
- `POST /api/0/dlp/policies/agents/{agent_id}/heartbeat`
- `GET /api/0/dlp/policies/agents/{agent_id}/desired`

## Workflow Example

1. Create draft:
`POST /api/0/dlp/policies`
2. Submit:
`POST /api/0/dlp/policies/{id}/submit`
3. Approve:
`POST /api/0/dlp/policies/{id}/approve`
4. Deploy:
`POST /api/0/dlp/policies/{id}/activate`

Every step is written to `policy_audit`.

## Deployment

- Service unit: `aw-dlp-policy-engine.service`
- DB path: `AW_DLP_POLICY_ENGINE_DB_PATH`
- Port: `AW_DLP_POLICY_ENGINE_PORT` (default `5601`)
- Ansible role: `ansible/roles/dlp-policy-engine/tasks/main.yml`

Recommended server deploy:

```bash
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml
```
