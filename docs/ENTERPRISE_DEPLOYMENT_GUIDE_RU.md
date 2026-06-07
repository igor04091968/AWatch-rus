# Enterprise Deployment Guide

Документ описывает варианты внедрения AWatch-rus в инфраструктуре заказчика.

Граница: документ не добавляет новую функциональность и не заявляет
неподтвержденные collectors. Конкретная поставка должна фиксировать фактически
включенные источники, версии и smoke-результаты.

## Архитектура развертывания

Базовая схема:

```text
Endpoint hosts / existing telemetry sources
        |
        v
AWatch-rus backend and ActivityWatch-compatible data layer
        |
        +--> role-based portal
        +--> JSON API contracts
        +--> reports and Markdown exports
        +--> readiness, health and metrics endpoints
        +--> optional integrations where configured
```

Основные компоненты:

- Rust backend and portal runtime;
- Rust Agent baseline;
- ActivityWatch-compatible telemetry layer;
- role-based portals: `executive`, `manager`, `security`, `forensics`, `admin`;
- reports and evidence materials;
- readiness/smoke tooling;
- optional storage/integration components where configured.

## Минимальная инсталляция

Назначение: локальная проверка, demo или первичная экспертиза.

Компоненты:

- AWatch-rus backend/portal;
- demo dataset;
- screenshots and demo report;
- local smoke scripts.

Не требуется:

- массовая установка агентов;
- production ingestion;
- pfSense integration;
- external SIEM/syslog.

Ограничение: минимальная инсталляция не доказывает production sizing и не
заменяет пилот на данных заказчика.

## Пилотная инсталляция

Назначение: ограниченная проверка ценности на согласованном контуре.

Компоненты:

- backend/portal on-premise;
- Rust Agent baseline или уже принятые источники;
- Workforce KPI and Explainable KPI;
- UEBA Score v1;
- Risk Narrative;
- Executive Action Center;
- reports and smoke checks.

Пилот должен фиксировать:

- список включенных источников;
- период сбора;
- coverage expectations;
- ответственных за эксплуатацию;
- fallback режимы;
- критерии приемки.

## Рекомендуемая инсталляция

Назначение: регулярный промышленный мониторинг.

Рекомендуемый профиль:

- выделенный Linux host или VM/LXC для backend/portal;
- systemd-managed services;
- reverse proxy with TLS;
- отдельное хранилище для state/reports/evidence metadata;
- регулярный backup;
- мониторинг `/healthz`, `/readyz`, `/metrics`;
- smoke после обновления и перед демонстрацией;
- ограниченный административный доступ;
- documented rollback plan.

## Optional integrations

Опциональные компоненты:

- pfSense as optional addon / `contract_only`, если ingestion отдельно не
  включен и не принят;
- 1C analytics where configured;
- AD/LDAP as planned or deployment-specific integration;
- SIEM/syslog as future or deployment-specific integration;
- external storage where required by customer policy.

Optional не означает обязательную зависимость AWatch-rus core.

## Ограничения

AWatch-rus deployment guide не заявляет:

- полноценную DLP;
- полноценную SIEM;
- EDR/XDR;
- ML/LLM scoring;
- auto-remediation without manual control;
- готовый universal agentless provider для любой инфраструктуры.

## Связанные документы

- [DEPLOYMENT_TOPOLOGIES_RU.md](DEPLOYMENT_TOPOLOGIES_RU.md)
- [SIZING_GUIDE_RU.md](SIZING_GUIDE_RU.md)
- [BACKUP_AND_RECOVERY_RU.md](BACKUP_AND_RECOVERY_RU.md)
- [OPERATIONS_RUNBOOK_RU.md](OPERATIONS_RUNBOOK_RU.md)
- [SECURITY_HARDENING_RU.md](SECURITY_HARDENING_RU.md)
- [ENTERPRISE_ACCEPTANCE_CHECKLIST_RU.md](ENTERPRISE_ACCEPTANCE_CHECKLIST_RU.md)
