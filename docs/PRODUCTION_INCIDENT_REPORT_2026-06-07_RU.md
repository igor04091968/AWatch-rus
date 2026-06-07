# Итог расследования production-инцидента 2026-06-07

Документ фиксирует результат расследования нестабильности рабочего контура
AWatch-rus перед пилотной демонстрацией. Все значения ниже обезличены: реальные
IP-адреса, hostname, учетные записи, домены и приватные пути не приводятся.

## Краткий вывод

Причина инцидента была не в ClickHouse и не в портале. Узкое место находилось в
цепочке `ActivityWatch Server -> aw-worktime-api-rust -> worktime reports`.

`aw-worktime-api-rust` при построении отчетов запрашивал слишком большой объем
events из bucket `aw-worktime-sessions_<HOST>` через ActivityWatch HTTP API.
На рабочей SQLite-базе это приводило к холодным запросам с таймаутами,
росту memory/CPU у `aw-server-rust`, зависанию прогрева отчетов и периодическим
таймаутам executive API в портале.

## Наблюдаемые симптомы

- `/portal/api/health` мог оставаться зеленым за счет кэша, но
  `/portal/api/reports?role=executive` периодически уходил в долгий ответ или
  таймаут.
- `aw-worktime-api-rust` писал ошибки построения отчетов на запросах вида
  `/buckets/aw-worktime-sessions_<HOST>/events?limit=5000`.
- `aw-server-rust` накапливал высокое потребление памяти и CPU, после чего даже
  metadata-запросы к bucket могли отвечать медленно.
- У file-operation telemetry наблюдался рост `sendFailures` в health-событиях:
  это указывало на сбои HTTP-доставки событий наблюдения во время перегруза,
  а не на потерю или копирование самих файлов.
- `aw-worktime-autoheal` и `aw-worktime-ui-bridge` в исходной оркестрации были
  чувствительны к placeholder-host, если runtime env не был применен.
- Operations View показывал legacy `rdp_window`/`rdp_afk` как критические
  источники, хотя фактический Rust-путь сбора `worktime_sessions` и
  `watcher_afk` был свежим.

## Что было исключено

- ClickHouse: контейнер был доступен, health-check после повторного запуска
  проходил успешно, данные читались.
- Nginx/gateway: локальные portal endpoints отвечали.
- Ролевая модель портала: smoke-тесты подтвердили серверные запреты.
- Отсутствие данных 1C: не являлось причиной таймаута worktime reports.

## Корневая причина

1. Верхний лимит `AW_WORKTIME_EVENTS_LIMIT` фактически не позволял задать
   малое production-значение: нижняя граница была `1000`, а код дополнительно
   ограничивал запрос к ActivityWatch значением до `5000`.
2. Worktime reports фильтровали дневной диапазон уже после получения событий,
   поэтому холодный запрос зависел от стоимости чтения большого bucket через AW
   HTTP API.
3. Несколько timer/service-компонентов могли одновременно прогревать отчеты или
   запускать health/autoheal, усиливая нагрузку.
4. Legacy RDP freshness checks не учитывали, что текущий промышленный путь уже
   идет через Rust-сбор `aw-worktime-sessions_<HOST>` и локальный watcher AFK.

## Внесенные исправления

- В `worktime-api` разрешен меньший безопасный лимит events:
  `AW_WORKTIME_EVENTS_LIMIT` теперь ограничивается диапазоном `100..50000`.
- Production baseline установлен на:
  - `AW_WORKTIME_EVENTS_LIMIT=250`;
  - `AW_WORKTIME_AW_HTTP_TIMEOUT_SECONDS=6`;
  - `AW_WORKTIME_EVENTS_CACHE_TTL_SECONDS=300`;
  - `AW_WORKTIME_REPORT_CACHE_TTL_SECONDS=300`;
  - `AW_WORKTIME_REPORT_STALE_TTL_SECONDS=3600`.
- Legacy `rdp_window` и `rdp_afk` больше не создают критический источник
  Operations View, если свежие `worktime_sessions` и `watcher_afk` уже
  подтверждают рабочий Rust-путь.
- `aw-worktime-autoheal.service` и `aw-worktime-ui-bridge.service` теперь берут
  `AW_WORKTIME_HOST` из `/etc/activitywatch/aw-server.env`, а не из
  hardcoded placeholder.
- Таймеры worktime-цепочки разведены по более щадящему графику:
  - `aw-worktime-autoheal.timer`: 15 минут;
  - `aw-worktime-prewarm.timer`: 15 минут;
  - `aw-worktime-ui-bridge.timer`: 5 минут;
  - `aw-rus-healthd.timer`: 5 минут.
- Gateway orchestration `detmir-auto.timer` переведен на 30 минут, а service
  source зафиксирован на Rust binary с bounded timeouts.
- File-operation collectors проверены отдельно: активного роста `sendFailures`
  после стабилизации сервера не было, локальные очереди были пустыми. Для
  снятия ложного аварийного хвоста перезапущены только file-operation
  collectors в активных пользовательских сессиях и восстановлены штатными
  `ActivityWatch Launch [...]` задачами.

## Проверка после исправления

Подтверждено на рабочем контуре:

- failed systemd units: `0`;
- ClickHouse container health: `healthy`;
- `/portal/api/health`: `ok=true`;
- `/reports/worktime/today`: `200`;
- `/reports/worktime/management`: `200`;
- full prewarm: все URL завершились `200` без `build-error`;
- `node scripts/detmir-pilot-demo-smoke.mjs`: OK;
- `node scripts/detmir-portal-tabs-smoke.mjs`: OK;
- role gates: executive/manager/security/forensics ограничения подтверждены.
- file-operation telemetry после перезапуска: по одному collector-процессу на
  активную сессию, `queueDepth=0`, `sendFailures=0`, свежие события приходят в
  bucket `aw-file-operations_<HOST>`.
- collector-guard до исправления не контролировал наличие Rust
  file-operation collector в активных сессиях: после принудительной остановки
  он продолжал писать `status=ok actions=0`. Исправление добавило проверку
  `fileOperationsPresence`: если в активной сессии есть Rust DLP/browser
  collector, но нет Rust file-operation collector, guard запускает штатные
  `ActivityWatch Launch [...]` задачи.
- Исправление collector-guard проверено fault-injection: один file-operation
  collector был принудительно остановлен, следующий цикл guard выполнил
  recovery (`actions>0`), после чего `missingSessions=[]`, `queueDepth=0`,
  `sendFailures=0`.

Локальные проверки кода:

- `cargo fmt --all --check`;
- `cargo test -p worktime-api`;
- `cargo clippy -p worktime-api --all-targets -- -D warnings`;
- `cargo build --release -p worktime-api`;
- `git diff --check`.

## Остаточные наблюдения

- Исторические `sendFailures` в старых health-событиях остаются в bucket как
  архивная телеметрия. Текущее состояние после перезапуска collectors чистое:
  рост `sendFailures` не подтвержден.
- ActivityWatch SQLite остается чувствительным к широким историческим
  `events`-запросам. Для production-демо и пилота надо держать bounded limits,
  кэш и предварительный прогрев.
- Реальные runtime значения должны оставаться в private inventory/env, а не в
  tracked repository files.

## Итоговое решение

Инцидент закрыт как operational performance regression в worktime reporting
chain. Исправление принято в код, оркестрацию и установочный пакет. Пилотный
контур можно показывать при выполненном преддемо-прогреве и зеленом smoke.
