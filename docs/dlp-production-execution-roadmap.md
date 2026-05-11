# DLP Production Execution Roadmap

## Purpose

This document converts the high-level production DLP plan into an execution sequence with explicit phase boundaries, dependencies, deliverables, and acceptance gates.

Source plan:
- [dlp-production-plan-windows-10-19.md](/mnt/usb_hdd2/Projects/ActivityWatch-Russian/docs/dlp-production-plan-windows-10-19.md:1)

## Execution Rule

Execution order is based on dependency and operational value, not on the original stage numbering from the idea draft.

## Phase Table

| Phase | Name | Depends on | Primary outcome |
|-------|------|------------|-----------------|
| 01 | Policy Engine | None | Central policy control plane with endpoint fallback |
| 02 | Content Analysis | 01 | Dictionary, regex, and OCR enrichment |
| 03 | Admin Tooling | 01 | Health checks and operator CLI |
| 04 | SIEM/SOAR Integrations | 01, 03 | External incident delivery and alerting |
| 05 | Case Management | 01, 03 | Investigation workflow and audit trail |
| 06 | Compliance Reporting | 01, 02, 03, 05 | Periodic reports for 152-FZ operations |

## Phase 01: Policy Engine

**Goal**
Build the server-side policy control plane without breaking endpoint autonomy.

**Deliverables**
- `aw-server/dlp-policy-engine/` service package
- SQLite-backed policy/version storage
- Active policy API
- rollback and backup flow
- endpoint `server/local/cached` policy modes
- Ansible deployment role and server integration

**Acceptance**
- Policies can be created, activated, versioned, and rolled back.
- Endpoints keep detecting while the policy service is unavailable.
- Invalid policies cannot become active.
- Operators can tell which policy source is active on each endpoint.

**First tasks**
- Create service skeleton and schema model.
- Define SQLite schema for `policies` and `policy_versions`.
- Add active policy endpoint and local cache contract.
- Update endpoint collector with `-PolicyMode` and cache fallback.
- Add Ansible variables and service deployment.

## Phase 02: Content Analysis

**Goal**
Increase detection quality with practical Russian PII and OCR enrichment.

**Depends on**
- Phase 01 active policy delivery

**Deliverables**
- `152-fz-pdn.json`
- checksum validation module
- dictionary matcher
- regex packs for finance, contacts, and secrets
- OCR processor and server-side artifact enrichment
- endpoint policy fields for dictionary/regex/OCR

**Acceptance**
- Incidents can be enriched by dictionary and regex matches.
- SNILS/INN validation reduces false positives.
- OCR can be turned on and off via policy.
- Screenshot processing path is explicit and auditable.

**First tasks**
- Create checksum validator for INN and SNILS.
- Define server-side pack loading contract.
- Add policy fields for `dictionaryPack`, `regexPack`, and `ocrEnabled`.
- Implement OCR wrapper and artifact processing pipeline.
- Extend endpoint incident payload for OCR-bound artifacts.

## Phase 03: Admin Tooling

**Goal**
Replace ad hoc operational steps with one supported CLI and one health check path.

**Depends on**
- Phase 01 policy engine API

**Deliverables**
- `scripts/dlp-admin-cli.py`
- `scripts/dlp-health-check.py`
- health check coverage for API, service state, endpoint sync, and disk
- documented operational commands

**Acceptance**
- Operator can inspect policy, incident, case, and service state from CLI.
- Health checks have machine-readable exit codes.
- Critical failures are visible without manual DB access.

**First tasks**
- Define CLI command surface and argument model.
- Implement policy list/push and health check commands first.
- Add endpoint sync status probe.
- Add service/systemd state checks.
- Document standard operator usage.

## Phase 04: SIEM/SOAR Integrations

**Goal**
Export actionable incidents outside the AW UI.

**Depends on**
- Phase 01 policy engine
- Phase 03 admin tooling and health checks

**Deliverables**
- CEF exporter
- webhook sender
- systemd service/timer units
- Ansible deployment for integrations

**Acceptance**
- High-severity incidents reach syslog/webhook targets.
- Retry/backoff protects against transient delivery failure.
- Failed exports are visible through logs and health checks.

**First tasks**
- Define normalized export schema.
- Build CEF severity mapping.
- Implement webhook retry/backoff.
- Add integration service configs and timers.
- Extend health checks to include delivery status.

## Phase 05: Case Management

**Goal**
Introduce a practical incident-to-case workflow with immutable audit history.

**Depends on**
- Phase 01 policy engine
- Phase 03 admin tooling

**Deliverables**
- case API and schema
- SQLite case store
- `case_audit` append-only log
- UI hooks in `aw-ru-patch.js` and `aw-case-management-ui.js`

**Acceptance**
- Operator can create a case from a DLP incident.
- Case status changes preserve history.
- Evidence links remain attached through case lifecycle.

**First tasks**
- Define case model, status model, and audit table.
- Implement create/list/update case endpoints.
- Add incident-to-case action in UI.
- Expose related cases on incident view.
- Add CLI support for case creation and listing.

## Phase 06: Compliance Reporting

**Goal**
Generate scheduled DLP compliance reporting usable for 152-FZ operations.

**Depends on**
- Phase 01 policy engine
- Phase 02 content analysis
- Phase 03 admin tooling
- Phase 05 case management

**Deliverables**
- report generator
- HTML template
- PDF export via `weasyprint`
- monthly scheduler service/timer
- email delivery path

**Acceptance**
- Monthly report can be generated without manual data prep.
- Report includes incidents, channels, users, and case linkage where available.
- Scheduler health and last-success state are visible operationally.

**First tasks**
- Define report input model and time-period filters.
- Create HTML template and PDF renderer wrapper.
- Implement monthly scheduler.
- Add email delivery configuration.
- Extend health check for report freshness.

## Release Discipline

- Deploy server components behind feature flags first.
- Keep endpoint fallback local until server path is proven.
- Pilot on a subset of Windows `10-19` before wide rollout.
- Do not enable OCR or external exports by default on first deployment wave.

## Completion Standard

This roadmap is complete only when each phase has:
- deployed code
- Ansible coverage
- health checks
- operator documentation
- rollback notes
- a passed acceptance gate
