# Registry Product Passport

Статус: инженерный паспорт для подготовки AWatch-rus к будущей подаче в
реестр российского ПО.

Документ не является юридическим заключением и не утверждает, что продукт уже
принят или гарантированно будет принят в реестр.

## Наименование продукта

Публичное наименование:

- `AWatch-rus`.

Рекомендуемая формулировка:

```text
Программный продукт AWatch-rus.
```

## Назначение

AWatch-rus предназначен для операционного контроля активности, технического
аудита, управленческой аналитики, поддержки ИБ-проверок и внутренних
расследований.

Продукт позиционируется как Workforce-first платформа:

- Workforce Analytics;
- Security Analytics;
- Forensics;
- operational readiness и отчетность.

## Функциональные модули

Core-модули:

- Rust backend и portal runtime;
- Rust Agent baseline;
- HTML/HTMX portal;
- Role-based portals для `executive`, `manager`, `security`, `forensics`,
  `admin`;
- Workforce KPI;
- Explainable KPI;
- UEBA Score v1;
- Risk Narrative;
- Executive Action Center;
- reports и Markdown export;
- Demo Pack для безопасного показа заказчику.

Дополнительные контуры и optional addons описаны отдельно в
[REGISTRY_FUNCTIONAL_SCOPE_RU.md](REGISTRY_FUNCTIONAL_SCOPE_RU.md).

## Архитектура

Базовая архитектура:

```text
Rust Agent / existing telemetry sources
        |
        v
Rust backend + ActivityWatch data layer
        |
        +--> API contracts
        +--> server-rendered HTML/HTMX portal
        +--> reports / Markdown exports
        +--> evidence and investigation materials
        +--> readiness and smoke checks
```

Подробное описание: [REGISTRY_ARCHITECTURE_RU.md](REGISTRY_ARCHITECTURE_RU.md).

## Стек технологий

Основной стек:

- Rust;
- HTML/HTMX-compatible server-rendered portal;
- JSON API contracts;
- OpenAPI и TypeScript declarations;
- ActivityWatch data model;
- SQLite/JSONL/local state там, где это уже используется проектом;
- systemd-friendly runtime для серверных компонентов.

Вспомогательный стек:

- Ansible и install-kit tooling;
- Python для отдельных вспомогательных направлений, которые не являются ядром
  Rust-first runtime;
- Node.js/Playwright для smoke и screenshot tooling;
- Grafana/Prometheus/InfluxDB как интеграционный и витринный слой там, где он
  включен в конкретной поставке.

## Режим поставки

Текущий режим поставки:

- исходный код в репозитории;
- документация и runbooks;
- install-kit/release assets для проверяемых сборок;
- demo fixtures и screenshots без живых данных;
- шаблоны конфигурации без secrets.

Приватные параметры конкретного внедрения не входят в публичный репозиторий.

## Состав ПО

В состав AWatch-rus входят:

- `adk-rust/` Rust workspace;
- portal contracts and static assets;
- agent baseline;
- operational tools and smoke scripts;
- deployment/runbook documentation;
- demo-pack and registry-readiness documentation.

Сторонние компоненты и license inventory описаны в:

- [THIRD_PARTY_LICENSES_RU.md](../THIRD_PARTY_LICENSES_RU.md);
- [docs/THIRD_PARTY_LICENSES_RU.md](THIRD_PARTY_LICENSES_RU.md);
- [SBOM_RELEASE_CHECKLIST_RU.md](SBOM_RELEASE_CHECKLIST_RU.md).

## Системные требования

Минимальные требования зависят от выбранного профиля внедрения.

Pilot/local demo:

- Linux host для backend/portal;
- доступ к release binaries или Rust toolchain для сборки;
- браузер для portal;
- локальные demo fixtures для безопасного показа.

Enterprise/on-premise:

- серверный Linux host;
- systemd-compatible runtime;
- endpoint hosts для agent baseline или существующих источников;
- storage для отчетов, telemetry state и evidence metadata;
- сетевой доступ между источниками и backend по утвержденной схеме.

Точные требования фиксируются в install/deployment документации конкретного
релиза.

## Ограничения

AWatch-rus не заявляется как:

- сертифицированная DLP;
- полноценная SIEM;
- EDR/XDR;
- ML/LLM-платформа;
- средство автоматического принятия юридически значимых решений;
- средство auto-remediation без ручного контроля.

## Что не является частью ядра

Не являются core runtime:

- future React/TypeScript Enterprise UI;
- future Tauri Desktop Forensics;
- full SIEM/syslog integrations;
- обязательный pfSense ingestion;
- внешние SaaS-зависимости;
- Python helpers, которые используются только как вспомогательные инструменты,
  миграционные утилиты или legacy/support runtime.
