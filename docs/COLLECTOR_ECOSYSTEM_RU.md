# AWatch-rus Data Collection Framework

Документ фиксирует модель развития сборщиков данных AWatch-rus без заявления
несуществующего функционала.

AWatch-rus рассматривается как платформа:

```text
Source -> Provider -> Normalized Event/API Contract -> Backend -> Portal -> Report
```

Важно: наличие точки расширения не означает, что соответствующий collector уже
реализован. Статус каждого направления указан отдельно.

## Статусы

- `implemented` - есть рабочая реализация в репозитории или текущем runtime.
- `planned` - направление предусмотрено архитектурно, но не является готовым
  универсальным provider.
- `future` - возможное направление развития без готового контракта внедрения.
- `contract_only` - есть схема/API/fixture или модель данных, но нет заявления
  о включенном production ingestion.

## Текущее состояние продукта

| Компонент | Статус | Что есть сейчас | Evidence |
|---|---:|---|---|
| Rust Agent | `implemented` | `awatch-agent-rs` и `aw-windows-telemetry.exe` как Rust-first runtime для агентского сбора, guard/validation и отдельных Windows-путей. Глубина сбора зависит от платформы и текущего parity-этапа. | `adk-rust/crates/awatch-agent-rs`, `adk-rust/crates/aw-windows-telemetry`, `docs/AGENT_ARCHITECTURE_RU.md`, `docs/POWERSHELL_TO_RUST_ROADMAP_RU.md` |
| API Contracts | `implemented` | Портальный слой и Pilot v1 API имеют стабильные JSON-контракты для executive/workforce/security/forensics/ueba/reports; отдельные интеграции могут иметь статус `contract_only`. | `docs/PILOT_V1_RU.md`, `docs/ROLES_RU.md`, `docs/UEBA_SCORE_RU.md` |
| Portal | `implemented` | Rust server-rendered HTML + HTMX-compatible JSON API; ролевые представления Executive / Workforce / Security / Forensics. | `README.md`, `docs/PILOT_V1_RU.md`, `docs/PORTAL_RU.md` |
| Backend | `implemented` | Rust-first серверные helpers для health, worktime, DLP/evidence, install-kit tooling, portal contracts и report layer. | `adk-rust/README.md`, `adk-rust/RUNBOOK.md` |

## Архитектурно предусмотренные точки расширения

| Provider | Статус | Назначение | Граница честного заявления |
|---|---:|---|---|
| PowerShell Provider | `planned` | Агентless/legacy сбор на Windows через существующие PowerShell-скрипты или WinRM-оркестрацию. | В репозитории есть PowerShell scripts и rollback/fallback слой, но формальный универсальный provider Data Collection Framework не заявлен как готовый. |
| SSH Provider | `planned` | Агентless сбор с Linux/Unix/network hosts через SSH-команды, read-only probes и existing logs. | Есть операционные SSH/Rust wrappers и Linux remote worker docs, но нет готового универсального SSH provider для массового пилота без агентов. |
| Syslog Provider | `planned` | Прием событий от сетевых устройств, Linux hosts, DLP/security tools или существующих log sources. | Серверные DLP syslog/CEF направления существуют как интеграционные helpers; полноценный inbound syslog collector для всех источников не заявляется. |
| 1C Provider | `implemented` | File-based 1C analytics через Rust ingest/ClickHouse и связанные отчеты. | Реализованный контур относится к текущему file analytics сценарию; это не универсальный 1C-коннектор ко всем конфигурациям 1C. |
| pfSense Provider | `contract_only` | Контракты для firewall events, VPN events, traffic summary, top destinations. | Есть API/fixture/docs readiness, но реальный ingestion и SIEM-функции не заявляются. |
| VPN Provider | `future` | Обогащение расследований и workforce/security analytics VPN-событиями из корпоративных VPN-шлюзов. | Отдельный production provider не реализован. pfSense VPN events остаются частью `contract_only` readiness. |
| SCUD Provider | `future` | Сопоставление активности рабочего места с событиями физического доступа. | Нет реализованного СКУД collector, схемы конкретного вендора или production ingestion. |
| Future API Providers | `future` | Подключение корпоративных систем через стабильные API-контракты и нормализацию событий. | Направление архитектурно допустимо, но каждый API provider должен получать отдельный контракт, тесты и статус только после реализации. |

## Принципы расширения

1. Provider не должен менять существующие portal/API contracts без обратной
   совместимости.
2. Новые источники должны сначала отдавать обезличенный fixture и JSON schema,
   затем проходить ingestion tests.
3. Статус `implemented` присваивается только после наличия кода, тестов,
   документации и smoke-проверки.
4. Demo data не должны содержать реальные IP, hostname, логины, ФИО,
   подразделения заказчика или реальные события безопасности.
5. AWatch-rus не заявляется как SIEM, классический DLP, EDR/XDR или
   сертифицированная СЗИ.

