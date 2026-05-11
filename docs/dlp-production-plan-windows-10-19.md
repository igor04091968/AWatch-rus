# Production DLP Plan for Windows 10-19

## Goal

Build a reliable, easy-to-deploy, and maintainable production DLP system on top of the existing `AWatch-rus` platform, with Windows hosts `10-19` as the primary target scope.

This plan assumes the current baseline already exists:
- Windows collectors for endpoint/browser/email/file telemetry
- ActivityWatch-based ingestion and Web UI overlays
- Ansible deployment for AW server and Windows endpoints
- InnoSetup-based Windows install kit

## Scope

In scope:
- Centralized DLP policy lifecycle
- Advanced practical content analysis for Russian personal data
- SIEM/SOAR exports and notifications
- Case management for investigations
- Compliance reporting
- Administrative tooling and health checks

Out of scope for this phase:
- Full enterprise RBAC/SoD model
- Multi-tenant administration
- Heavy ML/UEBA
- Approval workflows more complex than basic policy rollback/case review

## Delivery Principles

- Keep deployment simple: Python + SQLite + systemd on server, PowerShell on endpoints.
- Default to last-known-good behavior on every critical component.
- Do not break current Phase-1/2 DLP behavior while adding server-side controls.
- Prefer additive rollout behind feature flags and config toggles.
- Every new service must have Ansible deployment, health checks, logs, and rollback notes.

## Current Baseline and Gap

Current AWatch-rus DLP already provides:
- Rule-based endpoint detection
- Incident buckets and review UI
- Basic enforcement for clipboard/USB/print
- Email/file/browser collectors
- Initial reliability hardening work

Main gap to production DLP:
- Policies are still too endpoint-local
- Content analysis is not centralized or rich enough
- Incident export/investigation/reporting chain is incomplete
- Health/operations model is not yet unified

## Stage 1: Policy Engine

### Objective

Centralize policy management and distribution without breaking endpoint autonomy.

### Files

- `aw-server/dlp-policy-engine/policy_service.py`
- `aw-server/dlp-policy-engine/policy_schema.py`
- `aw-server/dlp-policy-engine/policy_storage.py`
- `aw-server/dlp-policy-engine/policy_distributor.py`
- `aw-server/dlp-policy-engine/requirements.txt`
- `aw-server/dlp-policy-engine/dlp-policy-engine.service`
- `ansible/roles/dlp-policy-engine/tasks/main.yml`
- `docs/dlp-policy-engine.md`
- `windows/dlp-policy-client.ps1`

### Server responsibilities

- REST API:
  - `GET /api/0/dlp/policies`
  - `POST /api/0/dlp/policies`
  - `PUT /api/0/dlp/policies/{id}`
  - `DELETE /api/0/dlp/policies/{id}`
  - `GET /api/0/dlp/policies/active`
- SQLite-backed storage with version history in `policy_versions`
- Validation through Pydantic and JSON schema before activation
- Backup and rollback of active policy versions
- Heartbeat-aware policy distribution model

### Endpoint changes

Update `windows/dlp-endpoint-signals-collector.ps1`:
- add `-PolicyMode` with values `local` and `server`
- in `server` mode pull active policy every 5 minutes
- cache last valid server policy locally
- fallback to local cached policy if server is unavailable

### Acceptance criteria

- Policy can be created, versioned, activated, and rolled back through API.
- Endpoints continue working during policy engine outage.
- Invalid policy cannot become active.
- Endpoint logs clearly show source of active policy: `local`, `server`, or `cached`.

### Main risks

- Breaking current local-policy-only flow
- Partial rollout where server mode is enabled before engine is healthy
- Policy drift between server and endpoints

### Risk controls

- Feature flag: `aw_dlp_policy_engine_enabled`
- Default endpoint mode remains `local` until validation is complete
- Store active policy checksum/version on both server and endpoint

## Stage 2: Advanced Content Analysis

### Objective

Add practical, legally relevant detection quality without overengineering.

### 2.1 Dictionary packs for 152-FZ personal data

#### Files

- `aw-server/dlp-content-analysis/dictionaries/152-fz-pdn.json`
- `aw-server/dlp-content-analysis/checksum_validator.py`
- `aw-server/dlp-content-analysis/dictionary_matcher.py`

#### Required capabilities

- Detect:
  - INN
  - SNILS
  - Russian passport patterns
- Validate checksums where applicable to reduce false positives

### 2.2 Regex packs

#### Files

- `aw-server/dlp-content-analysis/regex-packs/financial.json`
- `aw-server/dlp-content-analysis/regex-packs/contacts.json`
- `aw-server/dlp-content-analysis/regex-packs/secrets.json`

#### Required capabilities

- Reusable grouped pattern packs
- Server-defined matching rules distributed via policy
- Match metadata attached to incidents

### 2.3 OCR for screenshots

#### Files

- `aw-server/dlp-content-analysis/ocr_processor.py`
- `aw-server/dlp-content-analysis/requirements.txt`
- `ansible/roles/dlp-content-analysis/tasks/main.yml`

#### Required capabilities

- Tesseract wrapper for screenshots in `incident_artifacts`
- OCR text sent through regex and dictionary pipeline
- OCR enrichment attached to the original incident

### 2.4 Endpoint integration

Update:
- `windows/dlp-policy.example.json`
- `windows/dlp-endpoint-signals-collector.ps1`

Add policy fields:
- `dictionaryPack`
- `regexPack`
- `ocrEnabled`

Endpoint behavior:
- load server-delivered dictionary/regex references
- perform checksum-aware validation for supported PII
- upload screenshots for OCR when enabled by policy

### Acceptance criteria

- Server can enrich incidents with dictionary, regex, and OCR findings.
- False positives are reduced through checksum validation.
- OCR can be disabled per policy without code changes.
- Screenshot upload path is explicit and logged.

### Main risks

- OCR cost and latency
- Privacy overreach from over-collecting screenshots
- Regex pack sprawl and poor maintainability

### Risk controls

- OCR disabled by default
- Artifact retention policy documented
- Pack ownership and naming convention enforced

## Stage 3: SIEM and SOAR Integrations

### Objective

Make DLP incidents operational outside the AW UI.

### 3.1 CEF exporter

#### Files

- `aw-server/dlp-integrations/cef_exporter.py`
- `aw-server/dlp-integrations/cef-config.yaml`
- `aw-server/dlp-integrations/cef-exporter.service`
- `aw-server/dlp-integrations/cef-exporter.timer`

#### Required capabilities

- Read normalized incidents from SQLite/PostgreSQL
- Convert incidents to CEF
- Send via syslog
- Map DLP severities to CEF severities

### 3.2 Webhook notifications

#### Files

- `aw-server/dlp-integrations/webhook_sender.py`
- `aw-server/dlp-integrations/webhook-config.yaml`

#### Required capabilities

- Notify on `severity=high`
- Retry with backoff
- Include incident details, source host, user, rule, and evidence link

### 3.3 Ansible integration

Update `ansible/deploy_aw_server.yml`:
- install Python dependencies
- deploy configs/services/timers
- manage enable/start state

### Acceptance criteria

- High-severity incidents can be exported to SIEM and webhook endpoints.
- Export failures are visible and retry safely.
- Timers/services are idempotently managed by Ansible.

### Main risks

- Duplicate exports
- Alert fatigue
- Silent delivery failure to external systems

### Risk controls

- Event ID based dedupe
- Severity thresholding
- Delivery logs and health checks

## Stage 4: Case Management

### Objective

Provide a practical investigation workflow without introducing a heavy IR platform.

### Files

- `aw-server/dlp-case-management/case_service.py`
- `aw-server/dlp-case-management/case_schema.py`
- `aw-server/dlp-case-management/case_storage.py`
- `aw-server/dlp-case-management/case-service.service`
- `install-kit-awindows-20260427-211240/aw-server/aw-case-management-ui.js`

### Required capabilities

- Create a case from an incident
- Attach evidence links
- Support statuses:
  - `open`
  - `investigating`
  - `resolved`
  - `closed`
- Support comments
- Maintain immutable audit records in `case_audit`

### UI integration

Update `install-kit-awindows-20260427-211240/aw-server/aw-ru-patch.js`:
- add `Create case` action in DLP review table
- add `Case Management` section in UI
- show linked cases on incident views

### Acceptance criteria

- An operator can create and track a case directly from a DLP incident.
- Evidence remains linked after status transitions.
- Case audit trail is append-only.

### Main risks

- UI debt in current patch overlay
- Weak evidence chain semantics
- Mixing incident review and case workflow logic

### Risk controls

- Keep case service isolated from core AW server
- Use immutable audit table
- Treat evidence as links/references first, not copied blobs

## Stage 5: Compliance Reporting

### Objective

Generate regular compliance-grade reporting for Russian personal data handling.

### Files

- `aw-server/dlp-compliance/report_generator.py`
- `aw-server/dlp-compliance/templates/152-fz-report.html`
- `aw-server/dlp-compliance/report-scheduler.service`
- `aw-server/dlp-compliance/report-scheduler.timer`

### Required capabilities

- Period incident report
- Leak-channel statistics
- User statistics
- PDF export via `weasyprint`
- Scheduled email delivery

### Acceptance criteria

- Monthly report can be generated unattended.
- Report includes traceable source metrics.
- Output is usable by operations/compliance without manual cleanup.

### Main risks

- Weak data quality in upstream incidents
- PDF rendering dependency issues
- Email delivery failures

### Risk controls

- Validate report inputs before generation
- Keep HTML template under version control
- Add health check for scheduler and last successful report

## Stage 6: Administrative Tooling

### Objective

Make the whole stack operable without manual database edits or ad hoc scripts.

### Files

- `scripts/dlp-admin-cli.py`
- `scripts/dlp-health-check.py`

### Required CLI functions

- `python3 dlp-admin-cli.py policies list`
- `python3 dlp-admin-cli.py policies push --host HOSTNAME`
- `python3 dlp-admin-cli.py incidents list --severity high`
- `python3 dlp-admin-cli.py cases create --incident-id ID`
- `python3 dlp-admin-cli.py health check`

### Health checks

- API endpoint availability
- endpoint reachability and policy sync state
- queue health if implemented
- disk space
- systemd service state

### Acceptance criteria

- Operator can inspect policy, incident, case, and service health from CLI.
- Health check has machine-readable exit status.
- All critical services are covered by a single operational runbook.

## Cross-Cutting Ansible Work

Update:
- `ansible/deploy_aw_server.yml`
- `ansible/group_vars/all.example.yml`
- `ansible/roles/dlp-policy-engine/tasks/main.yml`
- `ansible/roles/dlp-content-analysis/tasks/main.yml`

Required variables:
- `aw_dlp_policy_engine_enabled: true`
- `aw_dlp_policy_engine_port: 5601`

Ansible quality bar:
- idempotent
- rollback-aware
- systemd-managed
- config templated, not hand-edited in prod

## Execution Order

1. Stage 1: Policy Engine
2. Stage 2: Advanced Content Analysis
3. Stage 6: Administrative Tooling
4. Stage 3: SIEM and SOAR Integrations
5. Stage 4: Case Management
6. Stage 5: Compliance Reporting

Rationale:
- centralized policies are the control plane
- content analysis increases signal quality
- admin tooling is needed before broadening operations
- integrations, cases, and reports depend on stable normalized incidents

## Release Strategy

### Wave 1

- Deploy policy engine on AW server
- Keep endpoints in `local` mode
- Validate API, versioning, rollback

### Wave 2

- Enable `server` policy mode for a pilot subset of Windows `10-19`
- Validate cache/fallback behavior
- Measure heartbeat and policy freshness

### Wave 3

- Roll out dictionary/regex/OCR selectively
- Enable SIEM/webhook export
- Stabilize case management and reporting

## Definition of Done

The plan is considered implemented only when:
- all new services are deployed by Ansible
- endpoint fallback works under server outage
- policy activation/rollback is proven
- content analysis is documented and testable
- SIEM/webhook integrations are observable
- case workflow is usable from UI
- monthly compliance report is generated automatically
- admin CLI and health checks replace ad hoc operational steps

## Deliverables Checklist

- [ ] Policy engine service and API
- [ ] Endpoint policy client and cache/fallback
- [ ] SQLite policy versioning and rollback
- [ ] Dictionary packs for 152-FZ
- [ ] Regex packs for financial/contact/secret data
- [ ] OCR processing pipeline
- [ ] CEF exporter
- [ ] Webhook sender
- [ ] Case management service and UI integration
- [ ] 152-FZ report generator and scheduler
- [ ] Administrative CLI
- [ ] Unified health check
- [ ] Ansible deployment coverage
- [ ] Operational documentation
