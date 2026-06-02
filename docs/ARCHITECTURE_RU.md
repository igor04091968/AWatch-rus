# DetMir: архитектура

Статус: подготовительный документ для продукта и реестра российского ПО.

Продуктовое имя: `DetMir`.

Техническая база и репозиторий: `AWatch-rus`.

## 1. Краткое описание

DetMir - программный комплекс операционного контроля, технического аудита,
мониторинга действий пользователей и автоматизации реагирования.

Архитектура построена вокруг ActivityWatch/AW-rus, Rust-сервисов DetMir,
Windows collectors, операторского портала, Grafana dashboards и evidence
workflow.

## 2. Логические уровни

```text
Endpoint layer
  Windows/RDP collectors
  Worktime/session collectors
  DLP/event collectors
  Evidence artifact sync

Server data layer
  AW-rus API
  ActivityWatch buckets
  SQLite/warehouse storage
  DLP/cases/policy storage

Processing layer
  Rust health/status helpers
  DLP aggregation/export
  Worktime/reporting
  SLO monitoring
  Hayabusa/offline enrichment

Operator layer
  DetMir Portal
  Grafana dashboards
  Telegram notifications

Automation layer
  Ansible
  systemd services/timers
  Windows scheduled tasks
  rollback/backups
```

## 3. Основные потоки данных

### Активность пользователя

```text
Windows session -> collectors -> AW-rus API -> buckets -> reports/Grafana/portal
```

### DLP/ИБ-инцидент

```text
Endpoint event -> DLP decision -> AW bucket -> warehouse -> portal/Grafana
```

### Evidence screenshot

```text
Endpoint artifact -> scheduled sync -> evidence upload API -> server storage
-> portal preview/download -> audit
```

### Health/SLO

```text
services/timers/API checks -> Rust helpers -> state files -> portal/Telegram
```

### Управленческая аналитика

```text
AW/1C/file telemetry -> processing/export -> ClickHouse/Influx/Grafana
```

## 4. Серверные компоненты

| Компонент | Роль |
|---|---|
| AW-rus server | Прием и хранение событий ActivityWatch. |
| detmir-status | Нормализованный статус контура. |
| detmir-check | Read-only проверка состояния. |
| detmir-dlp | Проверка DLP/ИБ контура. |
| detmir-auto | Safe automation и autoheal по allowlist. |
| detmir-portal | Web-интерфейс оператора. |
| detmir-grafana-check | Проверка Grafana dashboards/data freshness. |
| aw-slo-monitor | SLO samples и summary. |
| dlp-* Rust services | Aggregation, exporters, policy/case/compliance paths. |
| worktime-* Rust services | Worktime API, bridge, prewarm, autoheal, exporters. |

## 5. Endpoint-компоненты

| Компонент | Роль |
|---|---|
| AFK/window watchers | Базовая ActivityWatch активность. |
| browser domain collector | Домены/категории браузера. |
| worktime session collector | RDP/session presence. |
| DLP endpoint collectors | Clipboard, USB, print, file/email/browser signals. |
| evidence sync task | Доставка screenshots/artifacts на сервер. |

## 6. Evidence security path

Для evidence действует отдельная защитная логика:

- opaque `evidence_id`;
- no raw path serving;
- canonical path/root allowlist;
- Bearer token upload;
- `403` при upload без токена;
- PNG/JPEG magic validation;
- max-size limit;
- SHA-256 validation;
- atomic write;
- audit upload/view/download.

## 7. Runtime-принципы

1. Серверный контур автономен и не зависит от ноутбука администратора.
2. Rust используется для критичных helpers, где важны скорость, типизация и
   надежность.
3. Telegram bot runtime остается Python.
4. Legacy Python/shell сохраняется только там, где это оправдано совместимостью
   или низким выигрышем от переноса.
5. pfSense/network layer не меняется в app-level задачах без отдельного
   решения владельца.

## 8. Границы продукта

DetMir не заявляется как сертифицированная СЗИ, DLP, SIEM или EDR/XDR.

Продукт заявляется как платформа:

- операционного контроля;
- технического аудита;
- управления ИТ-инфраструктурой;
- контроля регламентов;
- расследования операционных и ИБ-событий.

## 9. Связанные документы

- `docs/DETMIR_UNIFIED_OPERATING_MODEL_RU.md`
- `docs/DETMIR_THREAT_MODEL_RU.md`
- `docs/ADMIN_GUIDE_RU.md`
- `docs/OPERATOR_GUIDE_RU.md`
- `docs/GRAFANA_DASHBOARDS_RU.md`
- `adk-rust/RUNBOOK.md`
