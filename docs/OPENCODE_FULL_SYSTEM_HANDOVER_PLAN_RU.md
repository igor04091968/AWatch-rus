# План передачи проекта агенту OpenCode

Документ нужен агенту, который должен самостоятельно разворачивать и
сопровождать полный контур AWatch-rus. Писать и действовать нужно просто:
сначала понять слой, затем развернуть, затем проверить, затем зафиксировать
результат.

## 1. Цель

Развернуть AWatch-rus как единый контур:

- сбор активности пользователей с Windows/RDP рабочих мест;
- учет рабочего времени и RDP-сессий;
- DLP/ИБ-сигналы: clipboard, USB, print, browser domains, email, file ops;
- ActivityWatch Server и русифицированный WebUI;
- worktime API и управленческие отчеты;
- Grafana dashboards поверх InfluxDB;
- ClickHouse-контур для файловой 1С, расследований, detections и cases;
- портал руководителя/ИБ/эксплуатации;
- проверяемый deploy через Ansible, Rust-бинарники, systemd и Windows tasks.

Главное правило: система считается развернутой только когда есть свежие данные,
открываются интерфейсы, проходят health checks и есть понятный rollback.

## 2. Простая модель системы

Представь систему как цепочку:

```text
Windows/RDP users
  -> Windows collectors / Rust agent
  -> ActivityWatch API buckets
  -> Rust services on AW server
  -> Worktime reports + DLP services + Influx exporters
  -> Grafana dashboards + Portal
  -> ClickHouse/1C analytics where configured
```

Если ломается ранний слой, поздний слой тоже будет пустым. Нельзя начинать с
Grafana, если ActivityWatch buckets пустые. Нельзя чинить портал, если
`aw-worktime-api` degraded. Нельзя чинить ClickHouse для worktime, потому что
worktime reports не зависят от ClickHouse.

## 3. Роли пользователей

Система должна закрывать четыре роли.

1. Руководитель:
   - видит, кто работал;
   - видит активное время по дням;
   - видит проблемные подразделения и риски;
   - получает простой вывод без технического шума.

2. ИБ:
   - видит DLP-инциденты;
   - видит evidence и screenshot artifacts;
   - видит DLP dashboards;
   - может разбирать кейсы без прямого доступа к сырой базе.

3. Эксплуатация:
   - видит свежесть buckets;
   - видит состояние сервисов;
   - видит failed units, timers, collector guard;
   - может безопасно перезапустить нужный слой.

4. Аналитик 1С:
   - видит аудит файловых баз 1С;
   - видит detections, timeline, cases;
   - видит состояние выгрузок и качество данных;
   - понимает, где данные реальные, а где proxy/fallback.

## 4. Обязательный функционал

### 4.1 ActivityWatch core

Нужно:

- `activitywatch-server` работает и слушает `:5600`;
- WebUI открывается;
- CORS/landing page настроены;
- buckets создаются и обновляются;
- SQLite не перегружен тяжелыми запросами.

Проверки:

```bash
curl -fsS http://<AW_SERVER_HOST>:5600/api/0/info
curl -fsS http://<AW_SERVER_HOST>:5600/api/0/buckets | jq 'keys'
systemctl status activitywatch-server --no-pager
```

### 4.2 Windows/RDP сбор

Нужно:

- Windows toolkit установлен;
- есть `deployment-config.json`;
- есть scheduled tasks `ActivityWatch Launch [...]` и `ActivityWatch Recovery`;
- Rust collector guard работает, PowerShell fallback остается только как
  fallback;
- `validate-deployment.ps1` возвращает `overallOk=True`;
- есть свежие buckets:
  - `aw-worktime-sessions_<HOST>`;
  - `aw-watcher-window_<HOST>`;
  - `aw-watcher-afk_<HOST>` если AFK включен;
  - `aw-dlp-endpoint-signals_<HOST>`;
  - `aw-file-operations_<HOST>` если file ops включен.

Развертывание:

```powershell
.\windows\deploy-ensemble.ps1 `
  -ServerHost <AW_SERVER_HOST> `
  -ServerPort 5600 `
  -Domain <WINDOWS_DOMAIN_OR_HOST> `
  -Users user1,user2,user3
```

Проверки:

```powershell
.\windows\validate-deployment.ps1 | ConvertTo-Json -Depth 10
Get-ScheduledTask | Where-Object TaskName -like 'ActivityWatch*'
Get-Service AWatchRusCollectorGuard
```

### 4.3 Worktime reports

Нужно:

- `aw-worktime-api` работает на `:5610`;
- `/health` возвращает OK или понятный degraded;
- `/reports/worktime/management` строит HTML/JSON;
- report не считает служебные accounts и битые labels;
- stale cache включен для degraded path.

Проверки:

```bash
curl -fsS http://<AW_SERVER_HOST>:5610/health | jq
curl -fsS "http://<AW_SERVER_HOST>:5610/reports/worktime/management?format=json&host=<WINDOWS_HOSTNAME>&allow_stale=1" | jq '.status,.runtime'
systemctl status aw-worktime-api --no-pager
```

### 4.4 DLP

Нужно:

- DLP policy engine доступен;
- endpoint signals пишутся;
- incidents создаются;
- screenshots/evidence синхронизируются;
- case management и compliance работают, если включены;
- DLP health check зеленый или объясняет WARN/FAIL.

Проверки:

```bash
systemctl status aw-dlp-policy-engine aw-dlp-case-management --no-pager
curl -fsS http://<AW_SERVER_HOST>:5601/health || true
curl -fsS http://<AW_SERVER_HOST>:5600/api/0/buckets/aw-dlp-endpoint-signals_<HOST>
```

### 4.5 WebUI

Нужно:

- ActivityWatch WebUI не пустой;
- RU patch подключен;
- host sanitize script подключен;
- DLP review/rules UI доступен, если включен;
- worktime panel не ломает основной WebUI;
- browser cache не скрывает новую версию.

Развертывание:

```bash
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml
```

Проверки:

```bash
curl -fsS http://<AW_SERVER_HOST>:5600/ | head
curl -fsS http://<AW_SERVER_HOST>:5600/js/ru-patch-v5.js | head
curl -fsS http://<AW_SERVER_HOST>:5600/js/aw-host-sanitize.js | head
```

### 4.6 Portal

Нужно:

- портал работает как read-only рабочий кабинет;
- роли видят разные представления, но данные общие;
- `/api/health` показывает состояние источников;
- `/api/reports` возвращает KPI и markdown;
- portal не должен silently mutate AW/DLP/1C;
- внешняя публикация идет через gateway/auth, не через открытые raw ports.

Развертывание:

```bash
cd adk-rust
cargo build --release -p detmir-portal
cd ../ansible
ansible-playbook -i inventory.ini deploy_detmir_portal.yml
ansible-playbook -i inventory.ini deploy_proxmox_web_gateway.yml
```

Проверки:

```bash
curl -fsS http://<PORTAL_HOST>:8720/api/health | jq
curl -fsS http://<PORTAL_HOST>:8720/api/reports | jq '.status,.sources'
systemctl status detmir-portal --no-pager
```

## 5. Grafana + InfluxDB

### 5.1 Что должно быть

InfluxDB хранит агрегаты для Grafana:

- `aw_rdp_worktime_daily`;
- `aw_rdp_worktime_hourly`;
- `aw_rdp_worktime_summary_daily`;
- DLP measurements;
- health/self-test measurements.

Grafana должна показывать:

- главный AWatch-rus dashboard;
- RDP/user activity dashboard;
- DLP/security dashboard;
- DLP management dashboard;
- overview dashboard для владельца.

### 5.2 Deploy order

1. Убедиться, что InfluxDB доступен.
2. Убедиться, что write tokens заданы в private inventory/env.
3. Развернуть AW server exporters.
4. Запустить exporters вручную один раз.
5. Проверить, что points записались.
6. Импортировать/provision Grafana dashboards.
7. Запустить `detmir-grafana-check`.

Команды:

```bash
systemctl start aw-worktime-influx-exporter.service
systemctl start aw-dlp-influx-exporter.service
journalctl -u aw-worktime-influx-exporter.service -n 50 --no-pager
journalctl -u aw-dlp-influx-exporter.service -n 50 --no-pager

cd ansible
ansible-playbook -i inventory.ini deploy_grafana_check.yml
```

Проверки Grafana:

```bash
curl -u "$GRAFANA_USER:$GRAFANA_PASSWORD" \
  http://<GRAFANA_HOST>:3000/api/datasources/uid/influxdb_aw/health
```

Ожидание: datasource OK, dashboards открываются, panels не пустые, labels
пользователей нормализованы.

### 5.3 Правила для dashboard

- Не править только руками в Grafana UI; сначала править JSON/provisioning в
  репозитории.
- Для worktime не показывать machine accounts, битые Unicode labels и дубли
  регистра.
- Owner-facing aggregate должен называться понятным языком, например
  `Все сотрудники`, а не техническим словом `Команда`.
- После импорта проверить dashboard API и открыть страницу через gateway.

## 6. ClickHouse + файловая 1С

### 6.1 Назначение

Этот контур нужен, когда 1С файловая и нужен не только KPI, а audit stack:

```text
1C exports / reglog / host telemetry
  -> landing/*
  -> aw-1c-ingest-rust
  -> ClickHouse analytics_1c
  -> detections / timeline / cases / company intelligence
  -> Grafana + Portal + briefs
```

### 6.2 Что развернуть

- ClickHouse;
- Grafana datasource ClickHouse;
- schema из `clickhouse-1c/clickhouse/init/*.sql`;
- landing каталоги;
- ingest timer/service;
- detections SQL;
- company intelligence refresh;
- read-only 1C analytics API;
- dashboards из `clickhouse-1c/grafana/provisioning/dashboards/files/`.

### 6.3 Minimal local bootstrap

```bash
cd clickhouse-1c
cp .env.example .env
docker compose up -d
mkdir -p landing/{documents,postings,business_events,document_changes,companies,reglog,audit,host}
cp etl/config.example.yml etl/config.yml
```

### 6.4 Production ingest

```bash
cd adk-rust
cargo build --release -p aw-1c-ingest

/usr/local/bin/aw-1c-ingest-rust --root /opt/activitywatch/clickhouse-1c
clickhouse-client --queries-file /opt/activitywatch/clickhouse-1c/detections/insert_detections.sql
clickhouse-client --queries-file /opt/activitywatch/clickhouse-1c/detections/build_entity_timeline.sql
```

### 6.5 Проверки ClickHouse/1С

```bash
clickhouse-client --query "SHOW DATABASES"
clickhouse-client --database analytics_1c --query "SHOW TABLES"
clickhouse-client --database analytics_1c --query "SELECT count() FROM business_events"
clickhouse-client --database analytics_1c --query "SELECT count() FROM detections"
```

Ожидание:

- таблицы существуют;
- raw/normalized слои не пустые, если есть выгрузки;
- detections считаются;
- Grafana 1C dashboards открываются;
- API компании/brief отвечает read-only.

## 7. Модули и ответственность

| Слой | Где смотреть | За что отвечает |
| --- | --- | --- |
| Rust runtime | `adk-rust/crates/*` | production binaries, checks, exporters, portal, ingest |
| AW server | `aw-server/`, `ansible/deploy_aw_server.yml` | ActivityWatch, WebUI, worktime, DLP services |
| Windows | `windows/`, `ansible/deploy_aw_windows.yml` | collectors, tasks, guard, validation |
| Grafana/Influx | `grafana/`, `ansible/deploy_grafana_check.yml` | dashboards, freshness checks |
| 1C/ClickHouse | `clickhouse-1c/` | file 1C analytics, detections, cases |
| Portal | `adk-rust/crates/detmir-portal`, `docs/PORTAL_RU.md` | role views, reports, health |
| Gateway | `ansible/deploy_proxmox_web_gateway.yml` | external protected routes |
| Docs/runbooks | `docs/`, `adk-rust/RUNBOOK.md` | operating procedures |

## 8. Полный порядок развёртывания

### Шаг 0. Не ломать рабочий контур

Перед любыми изменениями:

```bash
git status --short --branch
git log --oneline -5
```

Если есть unrelated dirty tree, не откатывать его. Работать только с нужными
файлами.

### Шаг 1. Подготовить private конфигурацию

Проверить:

- `ansible/inventory.ini`;
- private group vars;
- Influx tokens;
- Grafana credentials;
- Windows host/user list;
- gateway host/auth;
- ClickHouse credentials;
- 1C export paths.

Нельзя коммитить реальные secrets.

### Шаг 2. Собрать Rust

```bash
cd adk-rust
cargo fmt --all -- --check
cargo build --release --workspace
cargo test -p detmir-core
cargo test -p detmir-portal
cargo test -p worktime-api
cargo test -p worktime-influx-exporter
```

Если workspace слишком большой, собирать targeted crates, которые нужны
текущему deploy.

### Шаг 3. Проверить Ansible syntax

```bash
cd ansible
ansible-playbook --syntax-check deploy_aw_server.yml
ansible-playbook --syntax-check deploy_aw_windows.yml
ansible-playbook --syntax-check deploy_detmir_portal.yml
ansible-playbook --syntax-check deploy_grafana_check.yml
```

### Шаг 4. Развернуть AW server

```bash
ansible-playbook -i inventory.ini deploy_aw_server.yml
```

После:

```bash
systemctl --failed --no-pager
systemctl status activitywatch-server aw-worktime-api --no-pager
curl -fsS http://<AW_SERVER_HOST>:5600/api/0/info
curl -fsS http://<AW_SERVER_HOST>:5610/health | jq
```

### Шаг 5. Развернуть Windows/RDP

```bash
ansible-playbook -i inventory.ini deploy_aw_windows.yml
```

Или вручную на Windows:

```powershell
.\windows\deploy-ensemble.ps1 -ServerHost <AW_SERVER_HOST> -ServerPort 5600 -Domain <DOMAIN> -Users user1,user2
.\windows\validate-deployment.ps1
```

После проверить свежесть buckets на AW server.

### Шаг 6. Запустить worktime chain

```bash
systemctl restart aw-worktime-api
systemctl start aw-worktime-prewarm.service || true
curl -fsS "http://<AW_SERVER_HOST>:5610/reports/worktime/management?format=json&host=<WINDOWS_HOSTNAME>&allow_stale=1" | jq
```

### Шаг 7. Запустить Influx exporters

```bash
systemctl start aw-worktime-influx-exporter.service
systemctl start aw-dlp-influx-exporter.service
journalctl -u aw-worktime-influx-exporter.service -n 50 --no-pager
journalctl -u aw-dlp-influx-exporter.service -n 50 --no-pager
```

Ожидание: `wrote ... points`.

### Шаг 8. Развернуть Grafana dashboards/checks

```bash
ansible-playbook -i inventory.ini deploy_grafana_check.yml
```

Проверить:

- datasource health OK;
- dashboard pages HTTP 200;
- worktime panels не пустые;
- DLP panels не пустые при наличии DLP events.

### Шаг 9. Развернуть 1C/ClickHouse

Если 1С контур нужен:

```bash
cd clickhouse-1c
docker compose up -d
clickhouse-client --queries-file clickhouse/init/00_database.sql
clickhouse-client --queries-file clickhouse/init/01_raw_tables.sql
clickhouse-client --queries-file clickhouse/init/02_core_tables.sql
clickhouse-client --queries-file clickhouse/init/03_views.sql
clickhouse-client --queries-file clickhouse/init/04_company_intelligence.sql
clickhouse-client --queries-file clickhouse/init/05_financial_reporting.sql
```

Затем включить ingest, detections и dashboards.

### Шаг 10. Развернуть portal/gateway

```bash
cd adk-rust
cargo build --release -p detmir-portal
cd ../ansible
ansible-playbook -i inventory.ini deploy_detmir_portal.yml
ansible-playbook -i inventory.ini deploy_proxmox_web_gateway.yml
```

Проверить:

```bash
curl -fsS http://<PORTAL_HOST>:8720/api/health | jq
curl -fsS http://<PORTAL_HOST>:8720/api/reports | jq '.status,.sources'
```

### Шаг 11. Финальная приемка

Минимум:

- `systemctl --failed` пустой на ключевых узлах;
- AW API отвечает;
- buckets свежие;
- Windows validation OK;
- worktime report OK;
- DLP health OK/WARN с понятной причиной;
- Influx exporters пишут points;
- Grafana datasource OK;
- dashboards открываются;
- portal health OK;
- ClickHouse/1C tables не пустые, если включен 1C контур;
- нет secrets в staged diff.

## 9. Диагностика по симптомам

### Portal пустой

1. Проверить `/portal/api/health`.
2. Проверить `aw-worktime-api`.
3. Проверить ActivityWatch buckets.
4. Проверить ClickHouse только если пустой именно 1C/security-events слой.

### Grafana пустая

1. Проверить Influx datasource health.
2. Проверить exporters logs.
3. Проверить Influx bucket/measurements.
4. Проверить dashboard JSON/provisioning.
5. Проверить time range и host variable.

### Worktime неверный

1. Проверить `aw-worktime-sessions_<HOST>`.
2. Проверить `AW_WORKTIME_EVENTS_LIMIT`.
3. Проверить user normalization.
4. Проверить stale cache.
5. Не трогать ClickHouse.

### DLP пустой

1. Проверить Windows collector/guard.
2. Проверить `aw-dlp-endpoint-signals_<HOST>`.
3. Проверить policy engine.
4. Проверить DLP case/evidence services.
5. Проверить DLP Influx exporter только для Grafana.

### 1C пустая

1. Проверить landing files.
2. Проверить `aw-1c-ingest-rust`.
3. Проверить ClickHouse schema.
4. Проверить detections SQL.
5. Проверить Grafana ClickHouse datasource.

## 10. Как агент должен работать

1. Сначала читать `AGENTS.md`.
2. Затем читать этот документ.
3. Для конкретного слоя читать профильный doc:
   - Windows: `docs/windows/deployment.md`;
   - Worktime: `docs/OPERATIONS_RUNBOOK_WORKTIME_RU.md`;
   - Portal: `docs/PORTAL_RU.md`;
   - Grafana: `docs/GRAFANA_DASHBOARDS_RU.md`;
   - 1C/ClickHouse: `clickhouse-1c/README.md`;
   - Rust migration/runtime: `adk-rust/RUNBOOK.md`.
4. Перед изменением фиксировать `git status`.
5. Перед deploy делать syntax/build checks.
6. После deploy делать runtime checks.
7. В ответе пользователю писать:
   - что изменено;
   - какие команды выполнены;
   - что проверено;
   - что осталось рискованным или не проверено.

## 11. Запреты

- Не печатать secrets.
- Не коммитить private inventory/env.
- Не править production Grafana только руками без отражения в repo.
- Не перезапускать все сервисы подряд.
- Не трогать сетевой периметр/gateway/pfSense без отдельной команды.
- Не делать destructive DB operations без backup.
- Не считать `ansible --syntax-check` полной проверкой: нужна runtime проверка.
- Не считать открывшийся UI доказательством: нужны свежие данные.

## 12. Итоговая Definition of Done

Полная замена ручного оператора возможна только если агент умеет:

- поднять server и Windows collectors;
- проверить buckets и freshness;
- восстановить worktime report;
- запустить Influx exporters;
- импортировать/проверить Grafana dashboards;
- поднять ClickHouse/1C ingest;
- проверить portal health/reports;
- найти слой отказа по симптомам;
- сделать rollback по backup;
- написать короткий отчет без секретов.

Если один из пунктов не выполнен, система не считается полностью переданной
агенту.
