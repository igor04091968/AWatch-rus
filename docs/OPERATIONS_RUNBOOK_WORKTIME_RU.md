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
  "http://<AW_SERVER_HOST>:5610/reports/worktime/management?format=json&host=SHARKON2025&allow_stale=1" \
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
  http://<AW_SERVER_HOST>:5600/api/0/buckets/aw-worktime-sessions_SHARKON2025 \
  | jq '.metadata.end'
```

Проверить список bucket без чтения тяжелых событий:

```bash
curl -sS --max-time 5 http://<AW_SERVER_HOST>:5600/api/0/buckets | jq 'keys'
```

Если metadata свежая, а report degraded, вероятная причина - перегрузка чтения
events или временная недоступность ActivityWatch API. Не запускайте повторные
тяжелые запросы вручную без лимитов `--max-time`.

## Дубли пользователей в Grafana/Influx

Симптом: панели Grafana показывают одного сотрудника несколькими строками,
например `USER5` и `user5`, `Администратор` и `администратор`, или показывают
служебные/битые метки вроде `SHARKON2025$` и строк с `�`.

Причина: старые версии worktime exporter писали raw `username`/`userId` в tag
`user`, а Grafana группировала Influx series по этому сырому tag. Поэтому
варианты регистра, machine account и поврежденная OEM/Unicode строка становились
разными series. После исправления exporter пишет canonical tags, но старые
series остаются в диапазоне Grafana до истечения retention/range, поэтому Flux
queries должны фильтровать и схлопывать их.

Текущая canonical policy для DetMir RDP host:

- `USER1/user1`, `USER4/user4`, `USER5/user5` -> `user1`, `user4`, `user5`;
- `администратор` -> `Администратор`;
- users с suffix `$` исключаются;
- users, содержащие Unicode replacement char `�`, исключаются.

Кодовые точки, где должна сохраняться одинаковая нормализация:

- `adk-rust/crates/worktime-api/src/main.rs`;
- `adk-rust/crates/worktime-influx-exporter/src/main.rs`.

Проверка перед deploy:

```bash
cd <REPO_ROOT>/adk-rust
cargo fmt --all --check
cargo test -p worktime-api -p worktime-influx-exporter
cargo build --release -p worktime-api -p worktime-influx-exporter
```

Минимальный deploy с backup бинарников:

```bash
cd <REPO_ROOT>/ansible
export no_proxy="localhost,127.0.0.1,10.10.10.13,10.10.10.2,10.10.10.0/24"
export NO_PROXY="$no_proxy"

ts=$(date -u +%Y%m%dT%H%M%SZ)
ansible -i inventory.ini aw_server -m shell -a "set -e; sudo cp -a /usr/local/bin/aw-worktime-api-rust /usr/local/bin/aw-worktime-api-rust.bak.${ts}; sudo cp -a /usr/local/bin/aw-worktime-influx-exporter-rust /usr/local/bin/aw-worktime-influx-exporter-rust.bak.${ts}"
ansible -i inventory.ini aw_server -m copy -a "src=/home/igor/.cache/detmir-adk-rust-target/release/worktime-api dest=/tmp/aw-worktime-api-rust.new mode=0755"
ansible -i inventory.ini aw_server -m copy -a "src=/home/igor/.cache/detmir-adk-rust-target/release/worktime-influx-exporter dest=/tmp/aw-worktime-influx-exporter-rust.new mode=0755"
ansible -i inventory.ini aw_server -m shell -a 'set -e; sudo install -o root -g root -m 0755 /tmp/aw-worktime-api-rust.new /usr/local/bin/aw-worktime-api-rust; sudo install -o root -g root -m 0755 /tmp/aw-worktime-influx-exporter-rust.new /usr/local/bin/aw-worktime-influx-exporter-rust'
ansible -i inventory.ini aw_server -m shell -a 'set -e; sudo systemctl restart aw-worktime-api; sudo systemctl start aw-worktime-influx-exporter.service; systemctl is-active aw-worktime-api'
```

Grafana cleanup для старых Influx series:

- dashboard JSON: `grafana/detmir-rdp-user-activity-dashboard.json` и
  `grafana/detmir-aw-main-dashboard.json`;
- Flux должен фильтровать `user_id !~ /\$$/` и `user_id !~ /�/`;
- известные текущие accounts должны мапиться в canonical labels до grouping;
- grouping должен быть по `report_date,user` или `_time,user`;
- для схлопывания duplicate series использовать `max(column: "_value")`, чтобы
  не удваивать часы.

Если Grafana API import возвращает `403`, используйте provisioning/DB fallback:

```bash
scp grafana/detmir-aw-main-dashboard.json grafana/detmir-rdp-user-activity-dashboard.json igor@10.10.10.2:~/codex-dashboard-import/
ssh igor@10.10.10.2 'sudo pct push 201 /home/igor/codex-dashboard-import/detmir-aw-main-dashboard.json /etc/grafana/provisioning/dashboards/aw/detmir-aw-main.json --perms 0644'
ssh igor@10.10.10.2 'sudo pct push 201 /home/igor/codex-dashboard-import/detmir-rdp-user-activity-dashboard.json /etc/grafana/provisioning/dashboards/aw/detmir-rdp-user-activity.json --perms 0644'
ssh igor@10.10.10.2 'sudo pct exec 201 -- bash -lc "cp -a /var/lib/grafana/grafana.db /var/lib/grafana/grafana.db.bak.$(date -u +%Y%m%dT%H%M%SZ); systemctl restart grafana-server"'
```

Если provisioning не перезаписал существующие DB dashboards, перед изменением
сделать backup `/var/lib/grafana/grafana.db`, затем заменить только
`dashboard.data` rows по uid `detmir-aw-main` и `detmir-rdp-user-activity`.

Проверка после deploy:

- live panel `Вчера: активность по сотрудникам` возвращает только labels
  `user1`, `user4`, `user5`, `Администратор`;
- bad labels list пуст для `USER*`, `SHARKON2025$`, `администратор`, `�` и
  labels, начинающихся с `\`;
- Grafana dashboard открывается с HTTP `200`, HTML title содержит `Grafana`;
- `aw-worktime-api`, `grafana-server` и `aw-worktime-influx-exporter.timer`
  активны.

## Проверка лимитов

Проверить системные настройки:

```bash
systemctl cat aw-worktime-api
grep '^AW_WORKTIME_' /etc/activitywatch/aw-server.env
```

Ключевые параметры:

- `AW_WORKTIME_EVENTS_LIMIT` - верхний лимит чтения events из ActivityWatch.
  Для дневной управленческой аналитики значение должно покрывать рабочий день
  по всем активным сессиям. Для пилотного контура используется `5000`; малые
  значения вроде `250` допустимы только для аварийного degraded-smoke, иначе
  отчет будет построен по последнему хвосту событий, а не по полному дню.
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
  "http://<AW_SERVER_HOST>:5610/reports/worktime/management?format=json&host=SHARKON2025&allow_stale=1" \
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

Последний production rollback set после исправления canonical users
`2026-06-09`:

- `/var/lib/grafana/grafana.db.bak.20260609T013605Z`;
- `/usr/local/bin/aw-worktime-api-rust.bak.20260609T011956Z`;
- `/usr/local/bin/aw-worktime-influx-exporter-rust.bak.20260609T011956Z`.

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
