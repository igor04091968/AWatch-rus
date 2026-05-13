# DLP Runtime Chain Status 2026-05-13

## Verified production chain

Verified on `10.10.10.13`:

- `policy engine`
  - service: `aw-dlp-policy-engine.service`
  - health: `GET http://127.0.0.1:5601/healthz`
  - active policy: `policyId=1`, `default-policy`, `version=1`
- `case management`
  - service: `aw-dlp-case-management.service`
  - health: `GET http://127.0.0.1:5602/health`
  - runtime data present: case `id=1`
- `compliance reporting`
  - timer active: `aw-dlp-report-scheduler.timer`
  - artifacts present:
    - `152-fz-2026-05.html`
    - `152-fz-2026-05.json`
    - `pci-dss-2026-05.html`
    - `pci-dss-2026-05.json`
- `integrations`
  - timers active:
    - `aw-dlp-cef-exporter.timer`
    - `aw-dlp-webhook-sender.timer`
    - `aw-dlp-syslog-forwarder.timer`
  - recent journal runs are clean
  - current runtime result is `sent=0` / `delivered=0` because no new incidents were generated since the last seen bucket event
- `endpoint -> incident ingest`
  - `aw-dlp-endpoint-signals_SHARKON2025` fresh
  - `aw-dlp-incidents_SHARKON2025` exists and contains valid historical incidents
- `health/admin`
  - `/usr/local/bin/dlp-health-check --json` = `ok=true`
  - `/usr/local/bin/dlp-admin-cli.py health check` = policy/cases/aw OK

## Operational conclusion

Core chain is working:

`policy -> endpoint collectors -> incident bucket -> case management -> compliance -> integrations`

There is no confirmed production break in the server-side DLP chain.

## Bounded residual backlog

- external webhook/syslog/CEF destinations are configured and runnable, but current production evidence only shows clean timer execution with zero fresh incidents to export
- stale incident buckets must not be treated as failure by health-check if endpoint transport and policy/case services are healthy
- remaining work belongs to content-analysis completion and broader productization, not to server-side chain break repair
