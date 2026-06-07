# Runbook восстановления worktime reports

Документ описывает безопасную диагностику и восстановление цепочки
`ActivityWatch -> aw-worktime-api -> portal executive reports`.

ClickHouse не является обязательной зависимостью worktime reports. Не
перезапускайте ClickHouse для восстановления отчетов рабочего времени, если нет
отдельного подтвержденного отказа ClickHouse.

## Симптомы перегруза

- `/portal/api/reports?role=executive` открывается медленно или отвечает
  degraded/stale.
- `/portal/api/health` показывает degraded-состояние worktime source.
- `/reports/worktime/management` на `aw-worktime-api` возвращает
  `status=DEGRADED`.
- В журнале `aw-worktime-api` растут `aw_query_timeout_count` или
  `report_build_error_count`.
- ActivityWatch HTTP API отвечает медленно, не отвечает или держит SQLite под
  высокой нагрузкой.

## Быстрая диагностика

Проверить состояние сервисов:

```bash
systemctl status activitywatch-server aw-worktime-api --no-pager
systemctl status aw-worktime-ui-bridge.timer aw-worktime-autoheal.timer aw-rus-healthd.timer --no-pager
```

Проверить последние журналы:

```bash
journalctl -u aw-worktime-api -n 80 --no-pager
journalctl -u activitywatch-server -n 80 --no-pager
```

Проверить health worktime API:

```bash
curl -sS --max-time 5 http://<AW_SERVER_HOST>:5610/health | jq
```

Проверить отчет в безопасном bounded-режиме:

```bash
curl -sS --max-time 8 \
  "http://<AW_SERVER_HOST>:5610/reports/worktime/management?format=json&host=HOST-EXAMPLE&allow_stale=1" \
  | jq '.status,.stale,.runtime'
```

Проверить portal health:

```bash
curl -sS --max-time 8 http://<PORTAL_HOST>/portal/api/health | jq
curl -sS --max-time 12 "http://<PORTAL_HOST>/portal/api/reports?role=executive" | jq '.status,.sources'
```

## Проверка свежести bucket

Проверить metadata конкретного worktime bucket:

```bash
curl -sS --max-time 5 \
  http://<AW_SERVER_HOST>:5600/api/0/buckets/aw-worktime-sessions_HOST-EXAMPLE \
  | jq '.metadata.end'
```

Проверить список bucket без чтения тяжелых событий:

```bash
curl -sS --max-time 5 http://<AW_SERVER_HOST>:5600/api/0/buckets | jq 'keys'
```

Если metadata свежая, а report degraded, вероятная причина - перегрузка чтения
events или временная недоступность ActivityWatch API. Не запускайте повторные
тяжелые запросы вручную без лимитов `--max-time`.

## Проверка лимитов

Проверить системные настройки:

```bash
systemctl cat aw-worktime-api
grep '^AW_WORKTIME_' /etc/activitywatch/aw-server.env
```

Ключевые параметры:

- `AW_WORKTIME_EVENTS_LIMIT` - верхний лимит чтения events из ActivityWatch.
- `AW_WORKTIME_AW_HTTP_TIMEOUT_SECONDS` - timeout запросов к ActivityWatch API.
- `AW_WORKTIME_SOURCE_HTTP_TIMEOUT_SECONDS` - timeout внешних source-запросов.
- `AW_WORKTIME_REPORT_CACHE_TTL_SECONDS` - TTL fresh report cache.
- `AW_WORKTIME_REPORT_STALE_TTL_SECONDS` - TTL stale cache для degraded path.

Нормальная production-политика: bounded timeouts, ограниченный events limit,
stale cache включен. Нулевой stale TTL допустим только для специальных тестов,
но не для демонстрации или промышленного пилота.

## Безопасный restart

1. Зафиксировать текущий сигнал:

```bash
systemctl status aw-worktime-api activitywatch-server --no-pager
journalctl -u aw-worktime-api -n 120 --no-pager
curl -sS --max-time 5 http://<AW_SERVER_HOST>:5610/health | jq
```

2. Перезапустить только `aw-worktime-api`, если ActivityWatch отвечает, но
   портал получает degraded report:

```bash
systemctl restart aw-worktime-api
sleep 3
curl -sS --max-time 5 http://<AW_SERVER_HOST>:5610/health | jq
```

3. Перезапустить `activitywatch-server` только если ActivityWatch API не
   отвечает или SQLite явно перегружен:

```bash
systemctl restart activitywatch-server
sleep 5
systemctl restart aw-worktime-api
```

4. Прогреть отчет один раз:

```bash
curl -sS --max-time 12 \
  "http://<AW_SERVER_HOST>:5610/reports/worktime/management?format=json&host=HOST-EXAMPLE&allow_stale=1" \
  | jq '.status,.stale,.runtime'
```

5. Проверить портал:

```bash
curl -sS --max-time 8 http://<PORTAL_HOST>/portal/api/health | jq
curl -sS --max-time 12 "http://<PORTAL_HOST>/portal/api/reports?role=executive" | jq '.status'
```

## Rollback

Rollback нужен, если после обновления бинарника или env-настроек:

- fresh report не собирается;
- stale cache не отдается;
- `/health` не отражает degraded-состояние;
- портал зависает вместо bounded degraded response.

Порядок:

```bash
systemctl stop aw-worktime-api
cp /usr/local/bin/aw-worktime-api.prev /usr/local/bin/aw-worktime-api
systemctl daemon-reload
systemctl start aw-worktime-api
curl -sS --max-time 5 http://<AW_SERVER_HOST>:5610/health | jq
```

Если rollback касается env/drop-in:

```bash
cp /etc/systemd/system/aw-worktime-api.service.d/override.conf.prev \
  /etc/systemd/system/aw-worktime-api.service.d/override.conf
systemctl daemon-reload
systemctl restart aw-worktime-api
```

Перед rollback убедитесь, что backup-файлы действительно относятся к предыдущей
рабочей версии.

## Признаки успешного восстановления

- `/reports/worktime/management` отвечает HTTP 200 в bounded time.
- При свежей сборке `status` отсутствует или равен `OK`, `stale=false`.
- При временном отказе ActivityWatch API отдается `status=DEGRADED`, а не
  timeout.
- Если stale cache доступен, response содержит `stale=true` и
  `runtime.report_stale_served=true`.
- Если stale cache недоступен, response компактный, `stale=false`,
  `reason=report_unavailable`.
- `/health` у `aw-worktime-api` и `/portal/api/health` не маркируют систему как
  fully healthy при degraded reports.
- Счетчики `aw_query_timeout_count` и `report_build_error_count` перестают
  расти после восстановления ActivityWatch API.

## Smoke-тест degraded path

Локально, без обращения к рабочему контуру:

```bash
cd <REPO_ROOT>
cd adk-rust && cargo build -p worktime-api
cd ..
node scripts/worktime-degraded-smoke.mjs
```

Ожидаемый результат:

```text
worktime degraded smoke OK
```

Smoke проверяет:

- fresh report успевает построиться и прогреть cache;
- при недоступном ActivityWatch API отдается stale degraded response;
- при отсутствии stale cache отдается компактный degraded response;
- health отражает degraded-состояние;
- runtime-поля присутствуют в JSON.
