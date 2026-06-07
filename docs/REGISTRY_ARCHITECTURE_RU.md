# Registry Architecture

Документ описывает архитектуру AWatch-rus для registry-readiness пакета.

Граница: документ фиксирует текущее инженерное состояние и предусмотренные
точки расширения. Roadmap-направления не являются реализованными функциями.

## Current Architecture

```text
Rust Agent baseline / existing sources
        |
        v
Telemetry and ActivityWatch-compatible data
        |
        v
Rust backend
        |
        +--> API contracts
        +--> HTML/HTMX portal
        +--> role-based views
        +--> reports and Markdown exports
        +--> evidence and investigation materials
        +--> smoke/readiness checks
```

## Rust Backend

Backend реализует:

- portal routes;
- role-filtered JSON payloads;
- report generation;
- readiness and health payloads;
- security/forensics/workforce contracts;
- deterministic rule-based models.

Backend не добавляет ML/LLM и не выполняет автоматические блокировки.

## Rust Agent Baseline

Rust Agent baseline используется как основной целевой агентный слой:

- telemetry envelope;
- heartbeat and session signals;
- spool/retry behavior;
- demo-safe and privacy-aware contract shape;
- отсутствие передачи персональных данных в demo fixtures.

Подробно: [RUST_AGENT_BASELINE_RU.md](RUST_AGENT_BASELINE_RU.md).

## HTML/HTMX Portal

Текущий основной UI:

- Rust server-rendered HTML;
- HTMX-compatible JSON flows;
- static CSS/JS assets;
- role-based views;
- OpenAPI и TypeScript declarations для future UI consumers.

Dioxus не используется. React/Tauri не являются текущим UI.

## API Contracts

Contracts:

- `/api/reports`;
- `/api/executive`;
- `/api/workforce`;
- `/api/security`;
- `/api/forensics`;
- `/api/ueba`;
- `/api/risk/narrative`;
- `/api/actions`;
- `/api/pfsense`;
- `/api/contracts/openapi.json`;
- `/api/contracts/typescript.d.ts`.

Contracts описывают текущую форму данных и не должны использоваться для
заявления несуществующих collectors.

## Reports

Report layer включает:

- Workforce reports;
- Explainable KPI;
- Risk Narrative;
- Executive Action Center;
- Forensics reporting;
- Markdown export.

Отчеты являются decision-support материалами и требуют ручной интерпретации.

## Telemetry Pipeline

Текущий pipeline строится вокруг:

- Rust Agent baseline;
- ActivityWatch-compatible telemetry;
- existing logs/state where already implemented;
- report builders;
- readiness/smoke checks.

Agentless providers, расширенные syslog/SIEM и SCUD/VPN integrations остаются
roadmap/future, если отдельно не реализованы и не приняты.

## Optional Integrations

Опциональные направления:

- 1C analytics where already configured;
- AD/LDAP as planned/future validation layer;
- syslog/SIEM as planned integration direction;
- external storage as deployment-specific option;
- pfSense readiness as optional addon.

## pfSense Addon Boundary

pfSense описывается как необязательный addon:

- firewall events contract;
- VPN events contract;
- traffic summary contract;
- top destinations contract;
- readiness/status layer.

Если production ingestion отдельно не включен и не принят, статус:

```text
contract_only
```

pfSense не является обязательной зависимостью AWatch-rus core.

## Future UI Roadmap

Roadmap-only направления:

- React/TypeScript Enterprise UI;
- Tauri Desktop Forensics.

Эти направления не являются текущим claim и не входят в core Pilot v1.
