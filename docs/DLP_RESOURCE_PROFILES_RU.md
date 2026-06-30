# DLP resource profiles for DetMir

Дата фиксации: 2026-06-30.

Цель: сохранить стабильный Workforce/AW hot path на малом DetMir Proxmox
контуре и оставить DLP подключаемым модулем. Loki CT в текущем production
resource profile отключен намеренно и не является обязательной зависимостью
AWatch-rus.

## Профили

### `core_only`

Production default и аварийный/экономный профиль для DetMir.

- DLP runtime: выключен.
- DLP Influx exporter: выключен.
- DLP aggregators/report/syslog/webhook/CEF/case/evidence units: выключены.
- Loki/Promtail: выключены.
- Workforce, Worktime, ActivityWatch, ClickHouse 1C, Grafana core,
  Hayabusa/Security Finding Inbox: работают независимо от DLP.

Назначение: безопасное состояние при перегрузе CPU/RAM/IOPS или при ручном
отключении DLP.

### `light`

Операторский re-enable профиль для DetMir после проверки ресурсов: лёгкий DLP
режим без Loki и без Influx-heavy path.

- Разрешены `activitywatch-dlp-aggregator.timer` и
  `aw-dlp-ioc-refresh.timer`.
- `dlp-aggregator-rust` собирает ограниченный срез из bucket-ов
  `aw-file-operations_` и `aw-dlp-incidents_` в локальный
  `dlp_warehouse.sqlite` для последующей UEBA-корреляции.
- `detmir-dlp-warehouse-sync.timer` доставляет SQLite warehouse на portal host
  через атомарный snapshot, чтобы DetMir Portal/UEBA читали локальный файл, а
  не блокировали AW server hot path.
- Для агрегатора заданы короткий lookback, малый event limit, timeout,
  `CPUQuota` и `MemoryMax`.
- Evidence, screenshots, case management и exporters остаются выключенными.
- InfluxDB/Grafana/Loki не участвуют в hot path лёгкого DLP.
- Используется для ежедневной эксплуатации, когда нужны DLP-сигналы для UEBA,
  но нельзя нагружать Proxmox/Influx/Grafana/ClickHouse.

### `on_demand`

Временный режим для конкретного инцидента или окна проверки.

- Разрешены IOC refresh, policy engine, case management и evidence API.
- Influx exporter, CEF/syslog/webhook/report scheduler и aggregator остаются
  выключенными, если администратор отдельно не выбрал `full`.
- После окна проверки профиль должен быть возвращён в `core_only`.

### `full`

Только вручную, только после resource preflight.

- Может включать DLP Influx exporter, aggregator, reports, integrations,
  policy/case и evidence.
- На DetMir не является штатным production режимом.
- Запрещено включать автоматически при обычном deploy/recovery.

## Управление

На AW server:

```bash
sudo /usr/local/bin/detmir-dlp-runtime-control status
sudo /usr/local/bin/detmir-dlp-runtime-control set-profile core_only
sudo /usr/local/bin/detmir-dlp-runtime-control set-profile light
sudo /usr/local/bin/detmir-dlp-runtime-control set-profile on_demand
sudo /usr/local/bin/detmir-dlp-runtime-control set-profile full
sudo /usr/local/bin/detmir-dlp-load-guard
```

Перед каждым `set-profile` скрипт сохраняет rollback-снимок active/enabled
состояния DLP units:

```text
/var/lib/activitywatch/health/dlp-runtime-rollback.state
```

Откат к предыдущему состоянию:

```bash
sudo /usr/local/bin/detmir-dlp-runtime-control rollback
```

Важно: rollback восстанавливает только systemd active/enabled состояния DLP
units. Он не меняет retention, не удаляет данные и не включает Loki CT.

## Автоотключение при перегрузе

`detmir-dlp-load-guard.timer` запускает
`/usr/local/bin/detmir-dlp-load-guard`. Guard читает `/proc/loadavg`,
`/proc/meminfo` и `/proc/stat`; если load, свободная память или iowait выходят
за пороги несколько запусков подряд (`AW_DLP_GUARD_STRIKES_REQUIRED`, default
`3`), а DLP units активны, он переводит DLP в `core_only` через:

```bash
AW_DLP_DISABLED_REASON=auto_disabled_by_dlp_load_guard:<reason> \
  /usr/local/bin/detmir-dlp-runtime-control set-profile core_only
```

State и история пишутся в:

```text
/var/lib/activitywatch/health/dlp-light-guard-state.json
/var/lib/activitywatch/health/dlp-light-guard-history/
```

Единичный IO/load spike фиксируется как `observe_overload`, но DLP не
отключается до достижения порога подряд. Guard не перезапускает
ActivityWatch/портал, не меняет маршруты, не трогает ClickHouse/Grafana и не
включает Loki. Возврат из `core_only` в `light` делает администратор после
стабилизации контура и проверки Proxmox/AW/Influx/Grafana/ClickHouse load. Если
перегруз повторится, guard снова переведёт профиль в `core_only`.

## Доставка DLP warehouse на портал

Portal читает DLP-срез из локального
`/var/lib/activitywatch/dlp_warehouse.sqlite`. На разнесённом контуре DetMir
этот файл создаётся на AW server, поэтому используется лёгкий sync:

```bash
sudo systemctl start detmir-dlp-warehouse-sync.service
sudo systemctl status detmir-dlp-warehouse-sync.timer
sudo jq . /var/lib/activitywatch/health/dlp-warehouse-sync-state.json
```

Sync делает SQLite backup на AW server и атомарно заменяет локальный файл на
portal host. Он не запускает DLP evidence/case/exporters и не включает Loki.

## Ansible defaults

Для DetMir production defaults должны оставаться экономными и
самозащищающимися:

```yaml
aw_dlp_profile: "core_only"
aw_dlp_enabled: false
aw_dlp_influx_enabled: false
aw_dlp_light_collector_enabled: false
aw_dlp_light_guard_enabled: true
detmir_portal_dlp_profile: "core_only"
detmir_portal_dlp_module_enabled_override: true
```

Для временного возврата в лёгкий профиль:

```yaml
aw_dlp_profile: "light"
aw_dlp_enabled: true
aw_dlp_influx_enabled: false
aw_dlp_light_collector_enabled: true
aw_dlp_light_guard_enabled: true
detmir_portal_dlp_profile: "light"
detmir_portal_dlp_module_enabled_override: true
```

Все тяжёлые DLP component flags должны быть `false`, пока администратор явно не
выбрал `on_demand` или `full`.

## Resource preflight перед `full`

Перед временным включением `full` проверить:

- Proxmox host load и steal/wait;
- свободную RAM и swap pressure;
- IOPS/latency storage;
- ClickHouse health и backlog ingest;
- InfluxDB/Grafana health, если они участвуют в выбранном профиле;
- ActivityWatch `/healthz`, Worktime API и portal latency;
- отсутствие старого Loki CT в autostart.

Если любой core-сервис деградирует, DLP возвращается в `core_only`.

## Проверка

```bash
DETMIR_RESILIENCE_EXPECT_DLP_PROFILE=light \
DETMIR_RESILIENCE_EXPECT_LOKI_OFF=1 \
scripts/detmir_resilience_check.sh --repo
```

Live check на сервере в `light` должен показывать inactive для heavy DLP units
и Loki units. В `core_only` inactive должны быть все optional DLP units.

## Запрещённые утверждения

- Не заявлять, что AWatch-rus заменяет DLP/SIEM/EDR.
- Не заявлять, что Loki обязателен для DetMir production.
- Не заявлять DLP health OK, если DLP выключен.
- Не запускать автоматическое блокирование рабочих станций без approve/apply
  workflow.
