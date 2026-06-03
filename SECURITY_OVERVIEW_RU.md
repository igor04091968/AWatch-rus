# AWatch-rus: обзор системы для службы информационной безопасности

## Обзор системы

`AWatch-rus` в текущем состоянии — это не только русифицированный `ActivityWatch`, а полный production-контур контроля пользовательской активности, DLP-сигналов, управленческой отчетности и bounded forensic follow-up.

Архитектурно систему удобно рассматривать как **четыре основных operational tiers с выделенным forensic layer**:

1. `Windows Clients / RDP host`
   На рабочих станциях и RDP-хостах работают PowerShell-коллекторы, которые собирают активность и DLP-сигналы.
2. `Linux Server`
   Серверный контур `AW-rus` на Linux принимает события, хранит bucket-данные, отдает WebUI и server-side API.
3. `Integration Layer`
   Здесь живут policy engine, case management, SIEM/webhook/syslog/CEF интеграции, Telegram operator path, `1C`-аналитика и внешние poller'ы.
4. `Monitoring Stack`
   Grafana, Prometheus, SQL/ClickHouse аналитические слои и operator gateway для обзорных и управленческих экранов.
5. `Forensic Layer`
   Отдельный bounded DFIR-путь через `Hayabusa`, который используется для post-incident enrichment, а не как основной real-time detector.

Подтвержденный runtime для `DetMir`:

- `<AW_SERVER_HOST>` — основной `AW-rus` server, health, worktime/reporting, DLP server-side services, `Hayabusa` processing.
- `<GATEWAY_HOST>` — operator/gateway host, Telegram bot, web gateway, часть `1C` analytics runtime.
- `<WINDOWS_HOST>` — `HOST-EXAMPLE`, Windows/RDP host с collector toolkit.
- `<GRAFANA_HOST>` — Grafana.
- `<FIREWALL_HOST>` — `pfSense`, сетевой perimeter и VPN.

Ключевые потоки данных:

- endpoint collector -> `AW-rus` API -> `aw-dlp-endpoint-signals_*`, `aw-file-operations_*`, `aw-worktime-sessions_*`, `aw-dlp-incidents_*`;
- server-side policy/case/integration services -> operator workflows и compliance artifacts;
- worktime/management API на `:5610` -> management pages, executive summary, trend/source freshness;
- `1C` file telemetry -> ClickHouse/API/Grafana management contour;
- EVTX package -> `Hayabusa` intake -> case linkage / Telegram alert / bounded metadata.

## DLP функционал

### Endpoint Signals Collector

Файл: `windows/dlp-endpoint-signals-collector.ps1`

Реализует:

- мониторинг `clipboard`;
- мониторинг печати;
- мониторинг `USB`;
- загрузку локальной или server-side DLP policy;
- генерацию heartbeat и incident событий;
- transport queue на диске с lock-файлом и безопасным flush-потоком;
- telemetry по `queueDepth`, `eventsEnqueued`, `eventsFlushed`, `sendFailures`.

Для `action: "block"` реализованы активные меры:

- `clipboard` — очистка буфера обмена;
- `USB` — write-block через `Set-Disk -IsReadOnly`;
- `print` — отмена print jobs.

Важно:

- enforcement уже реализован, но его scope ограничен endpoint/email каналами;
- это не inline network DLP и не full-content gateway.

### Browser Domains Collector с категоризацией

Файл: `windows/browser-domains-native-collector.ps1`

Реализует:

- сбор доменов и web-контекста;
- нормализацию в `aw-detmir-web-category_*`;
- сопоставление доменов с policy rules;
- генерацию DLP incident событий по web-правилам.

Практическое ограничение:

- web-контур в текущей модели в первую очередь наблюдающий и аналитический;
- Telegram DLP toggle не превращает browser path в настоящий inline web-block.

### Email Outbound Collector

Файл: `windows/email-outbound-collector.ps1`

Реализует:

- мониторинг исходящей почты через Outlook COM и сетевые SMTP-сигналы;
- DLP-правила `endpoint.email[]`;
- reaction path для `action: "block"` через перемещение письма в Drafts в Outlook mode;
- privacy-preserving подход: тема и получатели могут храниться как hash/metadata, без постоянного чтения тела письма.

### DLP Aggregator

Файл: `scripts/aggregate_dlp_events.py`

Реализует:

- сбор `aw-file-operations_*` и `aw-dlp-incidents_*` в нормализованную БД;
- SQLite/PostgreSQL режимы;
- `PRAGMA journal_mode=WAL` для SQLite;
- основу для Grafana/SIEM-style reporting и поиска по событиям.

### DLP Policy API

Каталог: `aw-server/dlp-policy-engine/`

Реализует:

- централизованную активную policy;
- versioning и checksum;
- endpoint pull-model;
- API `GET /api/0/dlp/policies/active`;
- API `GET /api/0/dlp/policies/active/version`;
- agent heartbeat / desired state path.

Это уже production-usable server-side policy layer, но не enterprise policy suite с RBAC, approval matrix и криптографической подписью policy bundle.

## Управленческий мониторинг

### Management Report Layer на `:5610`

Файл: `aw-server/aw-worktime-api.py`

Контур включает:

- `GET /reports/worktime/today`;
- `GET /reports/worktime/management`;
- форматы `json`, `csv`, `html`;
- отдельную управленческую интерпретацию рабочего окна против календарной активности.

### Алиасы пользователей

Контур поддерживает:

- alias-файл сотрудников;
- owner/department mapping;
- manager-facing rollups по `owner` и `department`;
- нормализацию display names и руководителей.

### Actions с приоритетами

Management report строит:

- очередь действий;
- `critical/high` приоритеты;
- owner/department scope;
- executive interpretation уровня “что делать сегодня”.

### Source freshness monitoring

В management layer уже встроен контроль свежести источников:

- `aw-worktime-sessions_*`;
- `aw-watcher-window_*`;
- `aw-watcher-afk_*`;
- `aw-file-operations_*`;
- `aw-detmir-web-category_*`;
- смежные operational buckets.

Это важно с ИБ-позиции: система различает “данные есть, но пользователь не работал” и “данные stale, поэтому вывод ненадежен”.

### Executive summary и trend-анализ

Server-side management report уже выдает:

- summary по active/inactive users;
- actions queue;
- executive summary;
- trend за несколько дней;
- filtered management view по owner/department.

Практический смысл:

- это не просто тайм-трекер;
- это управленческий слой поверх telemetry, который помогает различать operational drift, real inactivity и collector degradation.

## Мониторинг и визуализация

### Prometheus Exporter

В проекте есть operational contour с Prometheus-compatible health/metrics logic и E2E проверками. Это используется для контроля server-side доступности и для внешних dashboard/alert workflows.

### Grafana дашборды

Version-controlled dashboard JSON находятся в `grafana/` и `clickhouse-1c/grafana/...`.

Основные экраны:

- RDP/worktime activity;
- DLP и ИБ overview;
- management/security boards;
- `1C` file telemetry;
- `1C` management board;
- financial reporting board.

### SQL Exporter для 1С KPI

Для `1C` контура реализован отдельный analytics stack:

- `ClickHouse`;
- ETL;
- company intelligence marts;
- management pages;
- Grafana dashboards.

Это read-only аналитический слой поверх telemetry и выгрузок, а не write-back path в production `1C`.

### E2E мониторинг

Контур уже содержит:

- `aw-health-check`;
- `scripts/dlp-health-check.py`;
- `check-aw-full.sh`;
- `check-aw-data.sh`;
- autoheal для worktime/reporting;
- внешний операторский контроль через Telegram bot.

### Proxmox Web Gateway

Развертывание: `ansible/deploy_proxmox_web_gateway.yml`

Назначение:

- единая внутренняя точка входа для operator/management pages;
- HTTPS reverse entrypoint;
- маршруты на Proxmox GUI, AW-rus UI, management reports, Grafana и `1C` pages.

## Надежность и отказоустойчивость

### WAL buffering

В проекте используются два близких, но разных механизма устойчивости:

- **server-side SQLite WAL** в policy/case/aggregation storage;
- **Windows collector disk queue** с lock-файлами и последующим flush в AW API.

Это снижает риск потери событий при кратковременной сетевой недоступности и при transient server-side сбоях.

### Graceful shutdown

Collector и server-side сервисы проектировались так, чтобы:

- не терять queued данные при штатной остановке;
- не держать transport lock во время network I/O;
- не блокировать весь pipeline одним зависшим POST.

### Health snapshots

Реализованы:

- `aw-rus-healthd.py`;
- state snapshots в `AW_RUS_HEALTH_STATE_DIR`;
- validation snapshots по Windows deploy/validation path;
- `Hayabusa` state snapshots (`latest-intake.json`).

Это дает operator и ИБ-команде не только “жив/мертв”, но и подтвержденное состояние последней валидации.

### Retry с exponential backoff

Реализован retry/backoff path минимум в:

- Windows transport queue flush;
- webhook sender;
- ряде integration/ingest контуров.

Это защищает от transient network/API failure, не превращая ошибку в постоянный incident storm.

### Предотвращение дубликатов процессов

В проекте есть отдельная работа против multi-instance regressions:

- lock-файлы для recovery/launch loops;
- проверки на stale queue + held lock;
- hardening deployment для Windows/RDP;
- частичное dedupe по incident/case semantics.

Практически это уменьшает риск process storm и ложных дублей telemetry.

### Ротация архивов деплоя

В Windows deploy toolkit и forensic/ingest контурах есть архивирование и ротация:

- deploy/install archives;
- backup/rollback roots;
- `Hayabusa` package archive и extracted payload archive;
- install-kit snapshots.

Это важно для расследований и rollback, потому что артефакты не исчезают после первой обработки.

## Интеграции

### Hayabusa forensic анализ

`Hayabusa` интегрирован как bounded DFIR enrichment:

- Windows экспортирует EVTX package;
- сервер принимает пакет в drop/inbox;
- `aw-hayabusa` строит forensic report;
- case linkage пишет bounded metadata;
- `high`-severity path может триггерить Telegram alert.

Ключевая граница:

- `Hayabusa` не является primary runtime detector;
- это forensic follow-up после инцидентов.

### pfSense poller

Файл: `pfsense/pfsense-aw-poller.py`

Реализует:

- внешний poller для `pfSense` API;
- отправку сетевой telemetry в `ActivityWatch`;
- включение firewall/VPN perimeter в единый observability contour.

### File-1C telemetry

Файл: `windows/export-upload-file-1c-telemetry.ps1`

Реализует:

- read-only telemetry по файловым базам `1C`;
- snapshots по `db size`, `reglog`, active locks, temp markers, scheduler activity;
- передачу данных в аналитический `ClickHouse` контур.

### TSJ Guardian Bot

Файл: `proxmox/tsj_guardian_bot.py`

Реализует:

- operator-facing health checks;
- DLP mode control;
- bounded auto-heal;
- status, support and investigation commands;
- human-readable operator menu для DLP и forensic path.

### MCP / PowerShell remote для DetMir

Документ: `docs/DETMIR_POWERSHELL_MCP_REMOTE_RU.md`

Реализует:

- operator/Codex remote path к Windows host;
- `SSH + powershell.exe` вместо `WSMan` для interactive operations;
- преднастроенный управляемый PowerShell path для `<WINDOWS_HOST>`.

## Деплой и эксплуатация

### Proxmox LXC

Базовый production deployment рассчитан на:

- Proxmox;
- LXC/CT для `AW-rus` server и смежных сервисов;
- отдельные runtime-host'ы для Grafana и operator/gateway paths.

### Ansible automation

Репозиторий содержит playbook'и для:

- server deployment;
- Windows deployment через `WinRM`;
- Grafana dashboard import;
- `pfSense` poller rollout;
- Proxmox web gateway rollout;
- bot/operator infrastructure.

### Windows deploy modes

Поддерживаются:

- `single-user`;
- `domain-users`;
- `ensemble`;
- standalone-service deployment mode;
- validation и hardening/recovery paths.

### Backup / rollback

В эксплуатационной модели уже предусмотрены:

- backup-first approach;
- deploy archives;
- rollback roots;
- `vzdump`/snapshot сценарии для LXC;
- forensic archive paths для intake payloads.

### Health validation publishing

Операционная модель уже поддерживает публикацию validation/health state:

- server-side health snapshots;
- Windows validation reports;
- transport freshness checks;
- operator-visible status через runbook и Telegram path.

## Безопасность и приватность

### Хранение секретов

Проектный принцип:

- реальные секреты не должны лежать в репозитории;
- используются `.example` и local secret files;
- для PowerShell/MCP отдельно оговорен локальный secret-config с правами `600`.

### Приватность данных

Ключевые ограничения и свойства:

- система не ведет постоянную запись экрана;
- OCR применяется к incident artifacts, а не к постоянному screen stream;
- email path не обязан хранить тело писем в открытом виде;
- management и `1C` слои строятся на read-only telemetry/выгрузках.

### Сетевая безопасность

Целевой operational подход:

- внутренний/VPN access вместо лишней публикации сервисов наружу;
- `pfSense` как perimeter control;
- operator access через gateway и управляемые entrypoints;
- `SSH` и `WinRM` разделены по назначению.

### Права доступа

Практическая модель прав:

- endpoint collectors и enforcement-функции требуют локальные Windows-права по своему каналу;
- часть enforcement logic требует admin/SYSTEM scope;
- server-side operator actions должны идти через ограниченные operational paths, а не прямой произвольный shell everywhere.

### TLS для Proxmox gateway

`Proxmox Web Gateway` разворачивается через `nginx` с TLS:

- HTTP redirect на HTTPS;
- `TLSv1.2` / `TLSv1.3`;
- отдельные certificate/key paths;
- по умолчанию возможен self-signed режим;
- для production рекомендуется заменить self-signed на корпоративный сертификат и держать gateway во внутреннем management contour.

## Вывод для ИБ

`AWatch-rus` уже дает практический DLP/monitoring/investigation contour для Windows/RDP и связанного Linux/operator слоя:

- endpoint и email DLP;
- management и source-freshness layer;
- case/integration/reporting path;
- bounded `Hayabusa` follow-up;
- production automation и health/autoheal.

При этом систему нужно честно оценивать как **open-source industrial scaffold с реализованными production-механиками**, а не как полностью завершенную enterprise DLP-платформу со встроенным RBAC, SSO и hardware-grade isolation.
