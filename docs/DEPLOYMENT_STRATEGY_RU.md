# AWatch-rus Deployment Strategy

Документ описывает стратегию внедрения AWatch-rus по уровням зрелости. Это
архитектурная модель, а не заявление о готовности всех способов сбора.

## Level 1 - Pilot

Назначение: быстрое обследование инфраструктуры и демонстрация управленческой,
эксплуатационной и ИБ-ценности без длительного проекта внедрения.

Целевые источники:

- PowerShell;
- SSH;
- Syslog;
- existing logs.

Статус: архитектурное направление.

Ключевое сообщение: пилот может быть проведен без массовой установки агентов
после реализации соответствующих providers и проверки их контрактов на
конкретной инфраструктуре.

Что уже есть:

- Windows/RDP toolkit и legacy/fallback PowerShell assets;
- Rust-first Windows runtime для части текущих путей;
- серверные Rust helpers;
- portal/API/report layer;
- install-kit и Ansible-оркестрация.

Что не заявляется как готовое:

- универсальный PowerShell Provider для любых Windows hosts;
- универсальный SSH Provider для любых Linux/network hosts;
- универсальный inbound Syslog Provider;
- agentless-пилот без предварительной адаптации источников.

## Level 2 - Enterprise

Назначение: регулярный промышленный мониторинг рабочих мест, RDP-сессий,
серверных контуров и качества данных.

Основной источник:

- Rust Agent.

Статус: основная целевая архитектура.

Текущая опора:

- `awatch-agent-rs` как единая модель `TelemetryRecord`;
- `aw-windows-telemetry.exe` для Windows runtime paths;
- server-side Rust services/helpers;
- API contracts и portal/report layer;
- spool/retry/backoff подход для устойчивой доставки там, где он реализован.

Граница заявления:

- глубина сбора зависит от платформы и реализованного collector path;
- legacy PowerShell assets могут оставаться rollback/reference слоем до полной
  parity;
- массовая эксплуатация требует проверки свежести buckets, agent health,
  ClickHouse/API status и smoke-тестов в конкретной инфраструктуре.

## Level 3 - Enterprise+

Назначение: корпоративная аналитическая платформа, которая связывает Workforce
Analytics, Security Analytics и Forensics с внешними корпоративными системами.

Потенциальные интеграции:

- AD;
- LDAP;
- 1C;
- SIEM;
- pfSense;
- VPN;
- SCUD.

Статус: частично реализовано / частично roadmap.

Матрица статусов:

| Интеграция | Статус | Комментарий |
|---|---:|---|
| AD | `planned` | Может использоваться для организационного контекста и ролей, но готовый универсальный AD provider здесь не заявляется. |
| LDAP | `planned` | Архитектурное направление для directory context; production provider требует отдельной реализации и тестов. |
| 1C | `implemented` | Есть file-based 1C analytics/ingest сценарий; не является универсальным коннектором ко всем 1C-конфигурациям. |
| SIEM | `future` | AWatch-rus не является SIEM. Возможна будущая интеграция как источник/потребитель событий через контракты. |
| pfSense | `contract_only` | Есть readiness contracts/fixture/API-заготовка; реальный ingestion не заявляется. |
| VPN | `future` | Может обогащать расследования и сетевой контекст, но отдельный provider не реализован. |
| SCUD | `future` | Возможное сопоставление с физическим доступом; готовый collector отсутствует. |

## Правила внедрения

1. Не включать новый источник в коммерческое описание как готовый, пока нет
   кода, контракта, тестов и документации.
2. Для пилота фиксировать список реально подключенных источников в acceptance
   checklist.
3. Для production использовать обратную совместимость API и миграции без
   потери данных.
4. Для внешних интеграций сначала готовить read-only режим, fixture и smoke,
   затем ingestion.

