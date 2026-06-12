# AGENTS.md

Operational rules for OpenCode/Codex agents in AWatch-rus.

## Defaults

- Rust is the primary runtime: use `adk-rust/`, build with `cargo build --release -p <crate>`, test with `cargo test -p <crate>`.
- Root scripts (`check-aw-data.sh`, `check-aw-full.sh`, `scripts/prod_rollout.sh`, install-kit helpers) are Rust-first wrappers with legacy fallback.
- Python is allowed only in `aw-server/dlp-content-analysis/`, `clickhouse-1c/ai/`, `clickhouse-1c/etl/`, `detmir-mcp/main.py`, `grafana-1c/`, `pfsense/`, `proxmox/tsj_guardian_bot.py`.
- Never add real secrets from `secrets/`, private `.env`, or host credentials.
- When auditing private/ignored files, report only path, secret type, and remediation. Never copy secret values into docs, logs, markdown, terminal summaries, commits, or handoff reports.

## Required Checks

- General: `scripts/quality-gate.sh`.
- Rust: targeted `cargo test -p <crate>`.
- Windows: parse PowerShell; CI also runs PSScriptAnalyzer on `windows/*.ps1`, `.psm1`, `.psd1`.
- Ansible: affected `ansible-playbook --syntax-check ...`.

## Map

- `adk-rust/`: operational crates.
- `aw-server/`: server install, env examples, RU WebUI patch, systemd.
- `windows/`: RDP deployment, collectors, recovery, validation.
- `ansible/`: deployment playbooks.
- `proxmox/`: CT/gateway/bot automation.
- `clickhouse-1c/`, `grafana-1c/`, `pfsense/`: integration stacks.
- `grafana/`: flat version-controlled dashboard JSON; use Ansible to import/check it.

## Entrypoints

Use `proxmox/create-ct.sh`, `proxmox/push-aw-artifacts.sh`, `aw-server/install_aw_server.sh`, `aw-server/apply_webui_ru_patch.sh`, `windows/deploy-ensemble.ps1`, and docs in `docs/preparation.md`, `docs/deployment.md`, `docs/runbook.md`, `docs/operations.md`.

## Incident Handling

OpenCode must handle AWatch-rus incidents as evidence-based operational triage,
not as guesswork from one red dashboard card.

### Assessment Basis

Assess every incident from these signals, in this order:

- **User impact:** portal/report/dashboard unavailable, stale, slow, or wrong;
  which role is affected: executive, manager, security, forensics, admin.
- **Data freshness:** ActivityWatch bucket `metadata.end`, collector heartbeats,
  Windows scheduled task recency, queue depth, and upload/send failure counters.
- **Service health:** systemd failed units, active timers, bounded HTTP checks,
  `/health` or `/api/health` responses, container health where relevant.
- **Pipeline layer:** identify the first broken layer in the chain
  `Windows/RDP collectors -> ActivityWatch buckets -> Rust services -> exporters
  -> Grafana/Portal -> ClickHouse/1C where configured`.
- **Risk/evidence:** DLP endpoint signals, incident candidates, evidence
  artifacts, UEBA/risk narrative inputs, coverage gaps, and security
  correlation indicators.
- **Blast radius:** one user/session/collector, one host, one service, one
  dashboard, or the full contour.
- **Recoverability:** known rollback, stale-cache availability, safe restart
  boundary, and whether a human approval is required.

Risk Narrative is only decision support. It can raise priority and explain
why a manual check is needed, but it does not prove a policy violation, DLP
incident, or SIEM finding by itself.

### Severity

Use this practical severity model:

- `P0`: data loss risk, auth/security boundary broken, raw private service
  exposed, production report chain unavailable with no stale fallback, or
  repeated collector process storms/memory pressure.
- `P1`: executive/security workflows degraded, fresh data missing for a critical
  host, DLP evidence sync broken, ClickHouse/1C ingest stopped, or portal health
  degraded with user-visible effect.
- `P2`: one collector stale, one dashboard/panel wrong, delayed timer, bad label
  normalization, missing noncritical evidence, or recoverable stale report.
- `P3`: documentation drift, cosmetic UI issue, non-production demo fixture,
  or a warning with fresh data still confirmed.

Escalate severity when the same symptom repeats after recovery, when coverage
is unknown, or when evidence contradicts dashboard status.

### Mechanisms To Use

Start with the repo wrappers before ad hoc probing:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
./check-aw-data.sh
./check-aw-full.sh
```

Then narrow by layer:

- ActivityWatch API: `/api/0/info`, `/api/0/buckets`, bucket metadata and recent
  events with explicit `no_proxy` and short `curl --max-time`.
- Worktime: `aw-worktime-api` `/health`, `/reports/worktime/today`,
  `/reports/worktime/management?allow_stale=1`, prewarm logs, stale-cache
  fields, `AW_WORKTIME_EVENTS_LIMIT`, and `aw_query_timeout_count`.
- Windows/RDP: `validate-deployment.ps1`, exact `ActivityWatch Launch [...]`
  scheduled tasks, `ActivityWatch Recovery`, collector guard state, session
  collectors, local queue depth, and send failure counters.
- DLP: `aw-dlp-policy-engine`, `aw-dlp-case-management`, `dlp-health-check`,
  `aw-dlp-endpoint-signals_<HOST>`, evidence artifact sync, policy audit, and
  case/compliance services.
- Portal/Gateway/Grafana: `/portal/api/health`, `/api/reports`, gateway
  `/healthz`, protected `/d/...` Grafana routes, role gates, and browser smoke
  scripts.
- ClickHouse/1C: only for file-1C/analytics incidents. Do not blame ClickHouse
  for worktime report failures unless the affected path explicitly uses it.

Use existing guards and bounded mechanisms before broad restarts:

- stale-cache and fail-closed worktime behavior;
- `aw-worktime-autoheal`, `aw-worktime-prewarm`, `aw-worktime-ui-bridge`,
  `aw-rus-healthd` timers;
- Windows collector guard and exact localized scheduled tasks;
- DLP evidence sync and health timers;
- targeted service restart only after evidence identifies the layer.

### DLP Rule Update System

Do not describe AWatch-rus DLP rules as manual local JSON entry, and do not
collapse all DLP updates into one mechanism. There are two related but separate
contours:

1. policy lifecycle and endpoint synchronization through the DLP Policy Engine;
2. automatic IOC/signature replenishment from the open-source Hayabusa/Sigma
   ruleset.

The centralized policy update contour is:

- Server service: `aw-dlp-policy-engine.service`, Rust binary
  `/usr/local/bin/aw-dlp-policy-engine-rust`, default API port `5601`.
- Storage: SQLite DB from `AW_DLP_POLICY_ENGINE_DB_PATH`, with policy records,
  policy versions, active policy pointer, rollback versions, and `policy_audit`.
- API contract:
  - `GET /healthz`;
  - CRUD: `/api/0/dlp/policies`;
  - active bundle: `GET /api/0/dlp/policies/active`;
  - active version/checksum: `GET /api/0/dlp/policies/active/version`;
  - approval lifecycle:
    `draft -> pending_approval -> approved -> deployed`;
  - workflow calls:
    `POST /submit`, `POST /approve`, `POST /draft`, `POST /activate`;
  - rollback: `POST /api/0/dlp/policies/rollback`;
  - audit:
    `GET /api/0/dlp/policies/audit?limit=N` and
    `GET /api/0/dlp/policies/{id}/audit?limit=N`;
  - endpoint sync:
    `POST /api/0/dlp/policies/agents/{agent_id}/heartbeat` and
    `GET /api/0/dlp/policies/agents/{agent_id}/desired`.
- Windows side is configured for server-driven policy mode:
  `aw_windows_policy_mode: "server"`,
  `aw_windows_policy_engine_enabled: true`,
  `aw_windows_policy_refresh_seconds: 300`, and policy engine host/port from
  Ansible group vars.
- Agents report their current policy version/checksum by heartbeat. The server
  compares it with the active deployed policy and returns `desired` with
  `refreshNow=true` when the endpoint must update.
- `dlp-admin-cli` is the operator CLI for read-side checks such as
  `policies list`, `policies active`, incident/case listing, and combined DLP
  health checks. It is not a replacement for the lifecycle API when changing
  policy state.

Automatic IOC/signature replenishment:

- Name it precisely as `DLP IOC Enrichment from Hayabusa/Sigma` or
  `Hayabusa Sigma IOC refresh pipeline`.
- Source rules come from the open-source GitHub ruleset
  `Yamato-Security/hayabusa-rules`, configured by
  `aw_dlp_ioc_rules_zip_url`.
- Deployment is controlled by `ansible/deploy_aw_server.yml` when
  `aw_dlp_ioc_enabled=true`.
- The refresh wrapper `/usr/local/bin/aw-dlp-ioc-refresh.sh` downloads the
  latest `hayabusa-rules` ZIP, unpacks Sigma YAML rules, and runs the Rust
  extractor `/usr/local/bin/aw-extract-ioc-from-sigma`.
- The Rust extractor is built from
  `adk-rust/crates/extract-ioc-from-sigma`; local/manual builds use
  `scripts/build_dlp_ioc_from_hayabusa.sh`.
- Extracted IOC-like values include process image suffixes, command-line
  substrings, original filenames, and SHA256 hashes. They are de-duplicated and
  emitted as `ioc_blacklist.json`, `ioc_blacklist.csv`, and
  `ioc_blacklist.sql`.
- Production artifacts live under `/opt/activitywatch/dlp-ioc/output` and are
  served by `aw-worktime-api` on `/dlp-ioc/ioc_blacklist.json`,
  `/dlp-ioc/ioc_blacklist.csv`, and `/dlp-ioc/ioc_blacklist.sql`.
- Windows DLP policy can consume this feed through the `ioc.source` field with
  format `hayabusa_sigma_v1`; endpoint health/heartbeat should expose loaded
  IOC state such as `iocRulesLoaded`.
- Runtime automation is `aw-dlp-ioc-refresh.service` plus
  `aw-dlp-ioc-refresh.timer` with interval `aw_dlp_ioc_refresh_interval`
  (default `6h`). Health/diagnostics should check this timer before assuming
  signatures are static or manually maintained.
- This Hayabusa/Sigma IOC pipeline enriches the DLP rule base automatically; it
  is not the same thing as hand-editing endpoint JSON and is also distinct from
  the server-side Hayabusa EVTX forensics runner.

Operational meaning:

1. To update rules, create or update a policy draft through the policy engine.
2. Submit it for approval, approve it, then activate/deploy it. Activation is
   allowed only from `approved`.
3. For policy changes, verify `active/version`, audit entries, Windows agent
   heartbeat/desired, and downstream DLP signals after endpoints refresh.
4. For automatic signature replenishment, verify
   `aw-dlp-ioc-refresh.timer`, the last `aw-dlp-ioc-refresh.service` run,
   non-empty `ioc_blacklist.json/csv/sql`, Worktime API `/dlp-ioc/...`
   exports, and Windows IOC load counters.
5. If a policy causes noise or misses, use policy rollback through the API; do
   not hand-edit endpoint policy files as the normal rollback path.

Manual edits of `C:\Program Files\AWatch-rus\windows\dlp-policy.example.json`
or `C:\ProgramData\AWatch-rus\dlp-policy.json` are diagnostic or emergency
fallback only. If such an edit is unavoidable, document it as configuration
drift and bring the rule back into the central policy engine.

### Response Workflow

1. Capture current state first: command, timestamp, host, service, and exact
   failing endpoint. Do not restart before collecting evidence unless the
   system is in active resource exhaustion.
2. Find the first broken layer. If buckets are stale, fix collectors before
   Grafana. If `aw-worktime-api` is degraded, fix/report that before portal.
3. Separate real outage from presentation drift: dashboards can be stale or
   mislabeled while buckets and services are healthy.
4. Apply the narrowest safe recovery: restart a collector/task/service, reduce
   unsafe limits, clear process storms, or restore a known-good binary/config.
   Back up config/binaries before replacement.
5. Verify with the same failing check plus one upstream and one downstream
   check. For collector incidents, require bucket freshness and guard/healthd
   consistency, not just one green command.
6. Record closure evidence: root cause, affected layer, action taken, commands
   run, post-check results, remaining risk, and rollback path.

### Safety Rules

- Old snapshots, memory, dashboards, and handoff notes are hints; live runtime
  evidence wins.
- Never expose passwords, tokens, private host credentials, private URLs, raw
  security events, or customer identifiers in incident writeups.
- Do not run broad deploys, full restarts, or `cargo build --workspace` during
  incident triage unless the scope demands it and rollback is clear.
- Do not treat `status=ok` as sufficient when freshness, queue depth, or
  coverage evidence says otherwise.
- For owner-facing reports, publish only protected gateway/Grafana routes, not
  raw `:5600`, `:5610`, `:8720`, or ClickHouse endpoints.
