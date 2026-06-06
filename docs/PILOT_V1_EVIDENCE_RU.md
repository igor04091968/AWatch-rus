# Pilot v1.0 Evidence

## Базовый контур

Базовый commit Pilot v1.0:
`067ad0939c3b8c34df8af9ff55bb219277d08341`.

Назначение evidence-документа: показать, какие артефакты подтверждают
готовность AWatch-rus Pilot v1.0 к демонстрации и приемке по ролям, API
contracts, UEBA Score v1 и pfSense readiness.

## Product boundary evidence

| Область | Артефакт | Что подтверждает |
| --- | --- | --- |
| Позиционирование | [PILOT_V1_RU.md](PILOT_V1_RU.md) | Workforce Analytics + Security Analytics + Forensics |
| Роли | [ROLES_RU.md](ROLES_RU.md) | `executive`, `manager`, `security`, `forensics`, `admin` |
| UEBA | [UEBA_SCORE_RU.md](UEBA_SCORE_RU.md) | rule-based scoring без ML/LLM |
| pfSense | [PFSENSE_INTEGRATION_RU.md](PFSENSE_INTEGRATION_RU.md) | readiness=`contract_only`, без SIEM/ingestion claims |
| Demo сценарий | [CUSTOMER_DEMO_SCENARIO_RU.md](CUSTOMER_DEMO_SCENARIO_RU.md) | 10-минутный показ заказчику |
| Преддемо | [DEMO_RUNBOOK_RU.md](DEMO_RUNBOOK_RU.md) | прогрев и порядок проверки перед показом |
| Gap analysis | [PILOT_GAP_ANALYSIS_RU.md](PILOT_GAP_ANALYSIS_RU.md) | известные риски и остаточные ограничения |

## API evidence

| Endpoint | Evidence | Приемочный смысл |
| --- | --- | --- |
| `/api/reports` | smoke + OpenAPI | общий отчет, role-filtered payload |
| `/api/executive` | OpenAPI + role gate smoke | Executive Dashboard |
| `/api/workforce` | OpenAPI + role gate smoke | Workforce Portal |
| `/api/security` | OpenAPI + role gate smoke | Security Portal |
| `/api/forensics` | OpenAPI + role gate smoke | Forensics Portal |
| `/api/ueba` | OpenAPI + tests | UEBA Score v1 |
| `/api/pfsense` | OpenAPI + fixture | pfSense readiness `contract_only` |
| `/api/contracts/openapi.json` | repository contract | стабильный OpenAPI contract |
| `/api/contracts/typescript.d.ts` | repository contract | стабильные TypeScript declarations |

Контрактные файлы:

- `adk-rust/crates/detmir-portal/src/contracts/openapi.json`;
- `adk-rust/crates/detmir-portal/src/contracts/typescript.d.ts`.

## Demo data evidence

Демонстрационные материалы:

- `docs/screenshots/01-executive-overview.png`;
- `docs/screenshots/02-risk-heatmap.png`;
- `docs/screenshots/03-security-view.png`;
- `docs/screenshots/04-operations-view.png`;
- `docs/screenshots/05-investigation-pack.png`;
- `docs/screenshots/06-markdown-report.png`;
- `docs/screenshots/07-product-architecture.png`;
- `docs/fixtures/pfsense-demo-events.json`.

Требования:

- не использовать реальные IP-адреса;
- не использовать реальные hostname;
- не использовать реальные логины;
- не использовать ФИО сотрудников;
- не использовать реальные подразделения заказчика;
- не использовать реальные события безопасности.

Для сетевых примеров допустимы только RFC 5737 диапазоны:

- `192.0.2.0/24`;
- `198.51.100.0/24`;
- `203.0.113.0/24`.

## Verification evidence

Обязательный набор команд:

```bash
cd adk-rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cd ..
git diff --check
node scripts/detmir-portal-tabs-smoke.mjs
```

Что подтверждают команды:

- `cargo fmt --all --check` - Rust-код форматирован.
- `cargo clippy --workspace --all-targets -- -D warnings` - нет warnings в
  workspace.
- `cargo test --workspace` - unit/integration tests проходят.
- `cargo build --workspace --release` - release-сборка workspace проходит.
- `git diff --check` - нет whitespace-ошибок в diff.
- `node scripts/detmir-portal-tabs-smoke.mjs` - портал, вкладки, роли и
  server-side role gates проходят smoke.

## Приемочный вывод

Pilot v1.0 готов к демонстрации, если:

- все команды verification evidence прошли успешно;
- role gates подтверждены smoke-тестом;
- UEBA Score v1 остается rule-based;
- pfSense readiness остается `contract_only`;
- документация не содержит обещаний SIEM, классического DLP, ML/LLM или
  реального pfSense ingestion.
