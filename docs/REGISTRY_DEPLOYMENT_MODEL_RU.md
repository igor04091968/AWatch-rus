# Registry Deployment Model

Документ описывает модели поставки и внедрения AWatch-rus для
registry-readiness пакета.

## On-premise Deployment

Основная целевая модель:

- размещение backend/portal на инфраструктуре заказчика;
- локальное хранение telemetry state, reports и evidence metadata;
- запуск серверных компонентов как systemd-friendly runtime;
- подключение endpoint sources через Rust Agent baseline или существующие
  источники, если они уже приняты в контуре;
- отсутствие обязательной внешней SaaS-зависимости для core runtime.

## Pilot Deployment

Pilot deployment предназначен для ограниченного показа и проверки ценности:

- role-based portal;
- Workforce KPI;
- Explainable KPI;
- UEBA Score v1;
- Risk Narrative;
- Executive Action Center;
- demo/reporting flow;
- readiness/smoke checks.

Пилот не должен заявляться как полный production security stack.

## Local Demo Deployment

Local demo deployment используется для безопасного показа:

- demo fixtures;
- demo screenshots;
- demo report;
- customer demo сценарии;
- smoke проверка dataset/screenshots/links.

Local demo не является production ingestion и не содержит живых данных.

## Agent Baseline Deployment

Rust Agent baseline:

- устанавливается на поддерживаемые endpoint hosts where applicable;
- отправляет telemetry по утвержденному contract;
- использует spool/retry behavior;
- не должен публиковать secrets или персональные данные в demo fixtures.

Подробно: [AGENT_DEPLOYMENT_RU.md](AGENT_DEPLOYMENT_RU.md).

## systemd / Docker

systemd-friendly модель является основной для серверных runtime-компонентов,
где это уже реализовано.

Docker может использоваться для отдельных инфраструктурных компонентов или
deployment-specific окружений, если это предусмотрено конкретной поставкой.

Наличие Docker в инфраструктуре не делает Docker обязательной зависимостью
core AWatch-rus.

## Install Kit

Install/release assets должны быть проверены:

- manifest completeness;
- checksums;
- absence of secrets;
- license/SBOM references;
- соответствие текущему release commit.

Связанные документы:

- [INSTALL_RU.md](INSTALL_RU.md);
- [FULL_DEPLOYMENT_MANUAL_RU.md](FULL_DEPLOYMENT_MANUAL_RU.md);
- [SBOM_RELEASE_CHECKLIST_RU.md](SBOM_RELEASE_CHECKLIST_RU.md).

## Pilot Version Limitations

Ограничения pilot deployment:

- не является сертифицированной DLP/SIEM/EDR;
- не содержит ML/LLM scoring;
- не выполняет auto-remediation без ручного контроля;
- optional integrations могут быть в `contract_only` или roadmap-статусе;
- полнота выводов зависит от coverage and data freshness.

## Rollout Evidence

Для реальной подачи и промышленного внедрения нужно подготовить:

- release tag;
- install guide;
- admin guide;
- user/operator guide;
- deployment checklist;
- smoke/e2e verification logs;
- SBOM and third-party licenses;
- screenshots and demo pack.
