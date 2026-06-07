# Roadmap Conformance Audit

Дата аудита: 2026-06-07.

Аудируемый срез: `origin/main` после TASK_011.

Цель: проверить соответствие roadmap, README, документации, API, портала,
отчетов, агента и smoke-проверок фактическому состоянию AWatch-rus. Новая
функциональность в рамках аудита не добавлялась.

## Executive Summary

AWatch-rus в целом соответствует Pilot v1 roadmap как Workforce-first платформа
с Security Analytics и Forensics-контуром. Подтверждены production hardening
endpoints, Explainable KPI, Risk Narrative, Executive Action Center, Rust Agent
baseline, demo pack, registry readiness package, enterprise deployment package
и pilot validation package.

Критичных conformance-блокеров на проверенном срезе не выявлено. Несколько
устаревших человеко-читаемых product-name claims были исправлены в runtime:
генерируемый отчет теперь начинается с `# AWatch-rus оперативный отчет`, а
headline, KPI label и CLI help используют публичное название AWatch-rus.

Закрытые housekeeping gaps Demo Freeze v1:

- TASK_001-TASK_004 получили явные секции `Выполнение` с артефактами,
  проверками и ограничениями.
- Для Risk Narrative создан отдельный документ `docs/RISK_NARRATIVE_RU.md`.
- Добавлен browser-level conformance smoke с Playwright и screenshots runtime
  artifacts.

Оставшиеся acceptance gaps:

- Полная production-приемка требует live validation на стенде заказчика:
  доступность, TLS/reverse proxy, источники данных, backup/restore и ownership
  действий.

## Overall Status

Статус: `ready for controlled pilot validation`.

Оценка соответствия roadmap: высокая, но не финальная production acceptance.

Что подтверждено проверками:

- Rust workspace собирается, форматируется, проходит clippy и unit tests.
- Production endpoints `/healthz`, `/readyz`, `/version`, `/metrics` работают на
  актуальном release-бинаре.
- `/api/reports`, `/api/workforce/kpi/explain`, `/api/risk/narrative` и
  `/api/actions` проходят smoke-проверку контрактов.
- Request id и correlation id возвращаются в HTTP headers.
- Query limits и role gates проверяются smoke.
- Demo, registry, deployment, screenshots, roadmap, reports и runbooks проходят
  статические smoke-проверки.

## Confirmed Implemented Items

### Production Hardening

Подтверждено:

- `GET /healthz`;
- `GET /readyz`;
- `GET /version`;
- `GET /metrics`;
- request id / correlation id;
- Prometheus text metrics;
- query limits for heavy API;
- report date range limits;
- role gates smoke;
- documentation in `docs/PRODUCTION_READINESS_RU.md`;
- smoke in `scripts/awatch-production-hardening-smoke.mjs`.

Evidence:

- `adk-rust/crates/detmir-portal/src/production/`;
- `adk-rust/crates/detmir-portal/src/main.rs`;
- `scripts/awatch-production-hardening-smoke.mjs`.

### Explainable KPI

Подтверждено:

- `GET /api/workforce/kpi/explain`;
- response model with `kpi_score`, `confidence`, `coverage`, `factors`,
  `top_applications`, `warnings`, `recommendations`;
- deterministic factors;
- role-filtered response;
- UI blocks `Почему такой индекс` / `Почему такой индекс активности?`;
- Markdown section `## Почему такой индекс`;
- documentation in `docs/EXPLAINABLE_KPI_RU.md`.

Evidence:

- `adk-rust/crates/detmir-portal/src/workforce_kpi_explain.rs`;
- `adk-rust/crates/detmir-portal/src/static/app.js`;
- `adk-rust/crates/detmir-portal/src/contracts/openapi.json`;
- `adk-rust/crates/detmir-portal/src/contracts/typescript.d.ts`.

### Risk Narrative

Подтверждено:

- `GET /api/risk/narrative`;
- deterministic rule-based model;
- `risk_score`, `risk_level`, `why`, `evidence`, `limitations`;
- Executive UI block `Риск-нарратив`;
- Markdown section `## Риск-нарратив`;
- OpenAPI and TypeScript contracts.

Evidence:

- `adk-rust/crates/detmir-portal/src/risk_narrative.rs`;
- `adk-rust/crates/detmir-portal/src/static/app.js`;
- `adk-rust/crates/detmir-portal/src/contracts/openapi.json`;
- `adk-rust/crates/detmir-portal/src/contracts/typescript.d.ts`.

### Executive Action Center

Подтверждено:

- `GET /api/actions`;
- rule-based action model;
- owner role, priority, deadline, reason codes and evidence;
- no auto-remediation;
- Executive and Security UI blocks;
- Markdown section `## Рекомендуемые действия`;
- documentation in `docs/EXECUTIVE_ACTION_CENTER_RU.md`.

Evidence:

- `adk-rust/crates/detmir-portal/src/executive_actions.rs`;
- `scripts/awatch-production-hardening-smoke.mjs`.

### Rust Agent Baseline

Подтверждено:

- crate `adk-rust/crates/awatch-agent/`;
- config loader;
- telemetry envelope;
- heartbeat;
- local spool;
- retry and dead-letter;
- `/healthz`;
- `/metrics`;
- structured JSON logging;
- unit tests.

Также подтверждено различение:

- `awatch-agent` - новый baseline core без мониторинга пользователя;
- `awatch-agent-rs` - текущий runtime для проверенных worktime/session задач.

Не обнаружено в baseline:

- keylogger;
- screenshot capture;
- clipboard capture;
- packet interception;
- kernel driver;
- EDR/DLP/ML/LLM behavior.

Evidence:

- `adk-rust/crates/awatch-agent/`;
- `adk-rust/crates/awatch-agent-rs/`;
- `docs/RUST_AGENT_BASELINE_RU.md`.

### Demo Pack

Подтверждено:

- `docs/demo/DEMO_SCENARIO_EXECUTIVE_RU.md`;
- `docs/demo/DEMO_SCENARIO_SECURITY_RU.md`;
- `docs/demo/DEMO_SCENARIO_FORENSICS_RU.md`;
- `docs/demo/DEMO_PACK_ACCEPTANCE_CHECKLIST_RU.md`;
- `docs/DEMO_REPORT_EXAMPLE_RU.md`;
- `docs/PILOT_VALUE_PROPOSITION_RU.md`;
- `docs/fixtures/pilot-v1-demo/demo-seed-data.json`;
- screenshots in `docs/screenshots/`.

Smoke подтвердил, что PNG не являются заглушками и ссылки валидны.

### Registry Readiness

Подтверждено:

- `docs/REGISTRY_PRODUCT_PASSPORT_RU.md`;
- `docs/REGISTRY_ARCHITECTURE_RU.md`;
- `docs/REGISTRY_FUNCTIONAL_SCOPE_RU.md`;
- `docs/REGISTRY_DEPENDENCY_STATEMENT_RU.md`;
- `docs/REGISTRY_DEPLOYMENT_MODEL_RU.md`;
- `docs/REGISTRY_COMMERCIAL_POSITIONING_RU.md`;
- `docs/REGISTRY_READINESS_CHECKLIST_RU.md`.

Core, optional and not claimed разделены. Есть explicit caveat, что документы
не являются юридической гарантией принятия в реестр.

### Enterprise Deployment Guide

Подтверждено:

- `docs/ENTERPRISE_DEPLOYMENT_GUIDE_RU.md`;
- `docs/DEPLOYMENT_TOPOLOGIES_RU.md`;
- `docs/SIZING_GUIDE_RU.md`;
- `docs/BACKUP_AND_RECOVERY_RU.md`;
- `docs/OPERATIONS_RUNBOOK_RU.md`;
- `docs/SECURITY_HARDENING_RU.md`;
- `docs/ENTERPRISE_ACCEPTANCE_CHECKLIST_RU.md`;
- `scripts/deployment-readiness-smoke.mjs`.

Sizing не заявлен как гарантия и требует проверки на инфраструктуре заказчика.

## Partially Implemented Items

- TASK_001 Pilot v1 Stabilization: результат фактически покрыт документами,
  smoke и Pilot v1 artifacts, но сам roadmap-файл не имеет явного статуса
  выполнения.
- TASK_002 Production Hardening: functionality and smoke confirmed, но
  roadmap-файл не содержит секцию `Выполнение`.
- TASK_003 Explainable KPI: functionality confirmed, но roadmap-файл не содержит
  секцию `Выполнение`.
- TASK_004 Risk Narrative: API/UI/report/contracts confirmed, но нет отдельного
  `docs/RISK_NARRATIVE_RU.md`, а roadmap-файл не содержит секцию `Выполнение`.
- Portal visual conformance: UI markers and API-backed blocks confirmed, но
  TASK_011 run не выполнял отдельный визуальный Playwright regression.

## Documentation-Only Items

- Registry readiness остается preparation package, not legal acceptance.
- Enterprise deployment docs задают target process, но production deployment
  требует отдельной приемки на стенде.
- Platform and collector ecosystem documents корректно описывают planned,
  future and contract-only направления, но не являются реализацией новых
  collectors.
- pfSense readiness остается optional / `contract_only`.

## Gaps

1. Обновить roadmap metadata:
   - добавить `## Выполнение` для TASK_001-TASK_004;
   - указать текущий статус и evidence files.
2. Создать отдельный `docs/RISK_NARRATIVE_RU.md` или явно сослаться в roadmap
   TASK_004 на документ, который заменяет dedicated Risk Narrative doc.
3. Добавить отдельный conformance smoke для roadmap/docs claims, чтобы TASK_011
   не оставался только ручным аудитом.
4. Выполнить live customer-stand validation:
   - portal URL;
   - TLS/reverse proxy;
   - role access;
   - `/api/reports`;
   - backup/restore;
   - data freshness.
5. Проверить public naming hygiene в старых исторических документах и, если они
   остаются GitHub-facing, привести их к AWatch-rus naming policy.
6. Зафиксировать release tag, release-specific SBOM and signed/checksummed
   artifacts перед registry/expert package.

## False Claims Found

Исправлено в рамках TASK_011:

- Markdown report title, executive headline, status KPI label and CLI help used
  stale internal product naming. Теперь человеко-читаемый вывод использует
  публичное название `AWatch-rus`.

Не обнаружено в README/Pilot v1 claims:

- claim полноценной DLP;
- claim полноценной SIEM;
- claim EDR/XDR;
- claim ML/LLM scoring;
- claim обязательного pfSense;
- claim готового React/Tauri UI;
- claim auto-remediation.

Оставшийся risk:

- В старых исторических документах и некоторых filename paths есть legacy
  naming. Это не product capability claim, но это снижает чистоту public GitHub
  presentation и должно быть отдельной cleanup-задачей.

## API Verification

Проверенные endpoints:

- `GET /healthz` - 200, JSON, response headers include `X-Request-Id` and
  `X-Correlation-Id`.
- `GET /readyz` - controlled 200/503 with JSON checks.
- `GET /version` - 200, includes `app_version` and `schema_version=pilot-v1`.
- `GET /metrics` - Prometheus text format, includes
  `awatch_http_requests_total` and `awatch_readyz_status`.
- `GET /api/reports` - covered by production smoke query limits and report
  payload tests.
- `GET /api/workforce/kpi/explain` - 200, numeric KPI and deterministic
  factors.
- `GET /api/risk/narrative` - 200, stable risk level and rule-based model.
- `GET /api/actions` - 200, actions array and no auto-remediation.

Role gates:

- manager -> `/api/security` returns 403 in smoke.
- Unit tests confirm executive/security/forensics report filtering.

Query limits:

- too large `page_size` rejected with `invalid_page_size`.
- too wide report date range rejected with `report_range_too_large`.

## Portal Verification

Confirmed by static UI code and tests:

- Executive Dashboard;
- Workforce KPI;
- Explainable KPI block;
- Risk Narrative block;
- Recommended Actions block;
- Security view;
- Forensics view;
- reports view;
- architecture page.

Evidence:

- `adk-rust/crates/detmir-portal/src/static/index.html`;
- `adk-rust/crates/detmir-portal/src/static/app.js`;
- `adk-rust/crates/detmir-portal/src/static/architecture.html`;
- `adk-rust/crates/detmir-portal/src/main.rs` tests.

Gap:

- TASK_011 did not require and did not run a visual Playwright screenshot
  regression. Use it before customer-facing UI freeze.

## Agent Verification

`awatch-agent` baseline conforms to TASK_005 scope:

- no user monitoring collectors;
- no clipboard/screenshot/keylogger/packet interception;
- no kernel driver;
- no DLP/EDR/ML/LLM behavior;
- heartbeat-only telemetry envelope;
- local spool and dead-letter;
- bounded retry/backoff;
- health and metrics endpoints;
- structured JSON logs.

`awatch-agent-rs` remains current runtime and has telemetry/session/worktime
tests. Documentation clearly separates baseline core from current runtime.

## Demo Pack Verification

Smoke results:

- demo docs exist;
- screenshots exist and are valid PNG files;
- demo dataset exists;
- Markdown links valid;
- sensitive scan for validation/demo files passed.

Screenshots verified:

- `01-executive-overview.png`;
- `02-risk-heatmap.png`;
- `03-security-view.png`;
- `04-operations-view.png`;
- `05-investigation-pack.png`;
- `06-markdown-report.png`;
- `07-product-architecture.png`.

## Registry Readiness Verification

Confirmed:

- core/optional/not claimed are separated;
- no legal guarantee of registry acceptance;
- remaining gaps are listed;
- SBOM is described as release-specific requirement;
- open-source dependencies are documented at package level.

Gap:

- release-specific SBOM and signed artifacts must be generated for final tag,
  not inferred from roadmap docs.

## Deployment Readiness Verification

Confirmed:

- deployment guide exists;
- topologies exist;
- sizing guide exists and includes caveats;
- backup/recovery exists;
- operations runbook exists;
- security hardening exists;
- enterprise acceptance checklist exists;
- deployment smoke passed.

Gap:

- restore test, sizing validation and live reverse proxy/TLS validation remain
  stand-specific acceptance tasks.

## Recommended Fixes

1. Add `## Выполнение` sections to TASK_001-TASK_004.
2. Add dedicated `docs/RISK_NARRATIVE_RU.md` or update TASK_004 to point to the
   accepted replacement document.
3. Add `scripts/roadmap-conformance-smoke.mjs` for future automated claim
   checks.
4. Add visual/browser conformance smoke before the customer demo freeze.
5. Clean old public-facing naming paths where legacy internal naming appears in
   GitHub-visible filenames or links.
6. Run live customer-stand validation and append evidence to pilot acceptance
   docs.

## Next Roadmap Corrections

- TASK_012: Roadmap metadata cleanup for TASK_001-TASK_004.
- TASK_013: Risk Narrative documentation closure.
- TASK_014: Public naming hygiene cleanup for historical docs and README links.
- TASK_015: Browser/visual conformance smoke for Executive, Security,
  Forensics and Reports views.
- TASK_016: Release tag, SBOM and signed artifact readiness.

## Checks

Commands executed from `adk-rust/` because the Rust workspace manifest is under
`adk-rust/Cargo.toml`:

- `cargo fmt --all --check` - OK.
- `cargo clippy --all-targets --all-features -- -D warnings` - OK.
- `cargo test --all` - OK.
- `cargo build --release` - OK.

Commands executed from repository root:

- `node scripts/deployment-readiness-smoke.mjs` - OK.
- `node scripts/pilot-validation-smoke.mjs` - OK.
- `AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs` - OK.
- `git diff --check` - OK.

Smoke note:

- The first live smoke attempt used a stale binary from `adk-rust/target/release`
  and correctly failed `/healthz`. The validated production smoke was rerun on
  the actual cargo release artifact from the configured cargo target cache.
