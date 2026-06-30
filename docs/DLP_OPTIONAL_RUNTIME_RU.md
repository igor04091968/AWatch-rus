# Optional DLP runtime for DetMir

Цель: DLP-контур должен оставаться подключаемым, но production default для
DetMir сейчас `core_only/disabled`. Возврат в лёгкий режим выполняется вручную
после resource check; при перегрузе guard снова переводит DLP в `core_only`.

Ресурсные профили и rollback-процедура описаны отдельно:
[DLP_RESOURCE_PROFILES_RU.md](DLP_RESOURCE_PROFILES_RU.md).

## Что отключается

Текущий production-профиль DetMir после 2026-06-30 prod hardening -
`core_only/disabled`:

- `AW_DLP_ENABLED=false`;
- `AW_DLP_PROFILE=core_only`;
- `AW_DLP_INFLUX_ENABLED=false`;
- optional DLP timers/services inactive/disabled;
- `detmir-dlp-load-guard.timer` остаётся enabled/active как защита на случай
  последующего operator re-enable;
- heavy DLP units, Influx exporter, evidence/case/report/integration units и
  Loki остаются выключенными.

Автоотключение выполняет `detmir-dlp-load-guard`: при превышении порогов
load/RAM/iowait он переводит DLP в `core_only` через
`detmir-dlp-runtime-control set-profile core_only`. После стабилизации контур
возвращается вручную командой `set-profile light`.

Штатный runtime off включает:

- `AW_DLP_ENABLED=false` на AW server;
- `DETMIR_DLP_ENABLED=false` в управляющем DetMir contour check;
- portal UI/API DLP-модуль может оставаться включённым для исторического
  SQLite/evidence-среза; это не запускает server-side DLP runtime;
- остановку DLP timers/services:
  - `aw-dlp-influx-exporter.timer`;
  - `activitywatch-dlp-aggregator.timer`;
  - `aw-dlp-report-scheduler.timer`;
  - `aw-dlp-syslog-forwarder.timer`;
  - `aw-dlp-webhook-sender.timer`;
  - `aw-dlp-cef-exporter.timer`;
  - `aw-dlp-ioc-refresh.timer`;
  - `aw-dlp-policy-engine.service`;
  - `aw-dlp-case-management.service`;
  - `detmir-portal-evidence.service`, если DLP evidence upload больше не нужен.

Worktime, ActivityWatch server, browser/window/AFK collection, Hayabusa,
Velociraptor findings ingest и 1C/ClickHouse core не считаются DLP runtime и
отдельно не отключаются.

Важно: отключение DLP runtime не удаляет DLP-контур из проекта. Это
эксплуатационный режим `disabled`, выбранный для production DetMir из-за
существенной нагрузки на виртуальную среду Proxmox, InfluxDB, Grafana,
ClickHouse и AW server. DLP должен оставаться подключаемым обратно через
описанный ниже enable-процесс, без переустановки продукта и без потери
исторических артефактов до отдельной retention/cleanup процедуры.

## Статистика перед отключением

На AW server:

```bash
sudo bash /usr/local/bin/detmir-dlp-runtime-control stats
sudo cat /var/lib/activitywatch/health/dlp-runtime-state.json
```

Из репозитория до деплоя:

```bash
sudo bash scripts/detmir_dlp_runtime_control.sh stats
```

JSON фиксирует:

- generated timestamp;
- effective mode;
- DLP unit `active/enabled/load` state;
- последние timestamps по DLP buckets для текущего host;
- причину отключения.

Каждый запуск дополнительно сохраняет неизменяемый снимок в
`/var/lib/activitywatch/health/dlp-runtime-history/`. При `disable` остаются
как минимум два снимка: `pre_disable` до остановки units и `disabled` после
остановки/disable/reset-failed.

## Отключение

На AW server:

```bash
sudo install -o root -g root -m 0755 scripts/detmir_dlp_runtime_control.sh /usr/local/bin/detmir-dlp-runtime-control
sudo sed -i \
  -e 's/^AW_DLP_ENABLED=.*/AW_DLP_ENABLED=false/' \
  -e 's/^AW_DLP_DISABLED_REASON=.*/AW_DLP_DISABLED_REASON=operator_disabled_to_reduce_influx_grafana_clickhouse_load/' \
  /etc/activitywatch/aw-server.env
sudo /usr/local/bin/detmir-dlp-runtime-control disable
sudo systemctl restart aw-worktime-api.service || true
```

Для portal:

```bash
sudo sed -i 's/^DETMIR_PORTAL_DLP_MODULE_ENABLED=.*/DETMIR_PORTAL_DLP_MODULE_ENABLED=false/' /etc/detmir-portal.env
sudo systemctl restart detmir-portal.service
```

Если переменной нет, добавьте ее в соответствующий env-файл отдельной строкой.

## Проверка после отключения

```bash
AW_DLP_ENABLED=false DETMIR_DLP_ENABLED=false /usr/local/bin/dlp-health-check --json
DETMIR_DLP_ENABLED=false detmir-dlp
DETMIR_DLP_ENABLED=false detmir-check --json
AW_DLP_ENABLED=false check-aw-full
```

Ожидаемое поведение:

- `dlp-health-check` возвращает `ok=true` и `dlp:mode=disabled`;
- `detmir-dlp` не открывает SSH health probe и возвращает disabled payload;
- `detmir-check` не проверяет DLP buckets;
- `check-aw-full` показывает DLP buckets как `SKIPPED`;
- DLP units остаются stopped/disabled;
- worktime/core проверки продолжают работать.

## Live evidence 2026-06-25

На DetMir/AW server выполнен controlled disable:

- `AW_DLP_ENABLED=false`;
- `AW_DLP_INFLUX_ENABLED=false`;
- `AW_DLP_DISABLED_REASON=operator_disabled_to_reduce_influx_grafana_clickhouse_load`;
- `AW_DLP_DISABLED_SINCE=2026-06-25`.

Зафиксированы evidence-снимки:

```text
/var/lib/activitywatch/health/dlp-runtime-history/dlp-runtime-current-20260625T083619Z.json
/var/lib/activitywatch/health/dlp-runtime-history/dlp-runtime-pre_disable-20260625T083637Z.json
/var/lib/activitywatch/health/dlp-runtime-history/dlp-runtime-disabled-20260625T083700Z.json
```

Результат:

- pre-disable active units included DLP Influx exporter, aggregator, report,
  integration, policy/case and evidence units;
- post-disable active/enabled DLP units: `0/0`;
- `dlp-health-check` returned `ok=true`, `dlp:mode=disabled`;
- `detmir-dlp` returned `ok=true`, `dlp:mode=disabled`;
- `check-aw-full` returned `DLP buckets ... SKIPPED`.

Отдельные non-DLP findings того же ручного прогона, до восстановления
`192.168.100.19`:

- AFK/window/worktime buckets were stale and require RDP collector/session
  recovery;
- WinRM from server side to `192.168.100.18:5985` was unreachable;
- these findings are outside the DLP runtime disable boundary.

## Возврат DLP

Для DetMir предпочтительно возвращать не весь DLP сразу, а лёгкий профиль.
Перед этим проверить load/RAM/iowait на Proxmox/AW/Influx/Grafana/ClickHouse.

```bash
sudo AW_DLP_DISABLED_REASON=operator_reenable_after_resource_check \
  /usr/local/bin/detmir-dlp-runtime-control set-profile light
sudo sed -i \
  -e 's/^AW_DLP_ENABLED=.*/AW_DLP_ENABLED=true/' \
  -e 's/^AW_DLP_PROFILE=.*/AW_DLP_PROFILE=light/' \
  -e 's/^AW_DLP_INFLUX_ENABLED=.*/AW_DLP_INFLUX_ENABLED=false/' \
  /etc/activitywatch/aw-server.env
```

Если профиль ухудшил состояние контура:

```bash
sudo /usr/local/bin/detmir-dlp-runtime-control rollback
```

`on_demand` и `full` включаются только вручную после отдельного resource
preflight:

```bash
sudo /usr/local/bin/detmir-dlp-runtime-control set-profile on_demand
sudo /usr/local/bin/detmir-dlp-runtime-control set-profile full
```

Для portal:

```bash
sudo sed -i 's/^DETMIR_PORTAL_DLP_MODULE_ENABLED=.*/DETMIR_PORTAL_DLP_MODULE_ENABLED=true/' /etc/detmir-portal.env
sudo systemctl restart detmir-portal.service
```

После включения выполнить:

```bash
/usr/local/bin/dlp-health-check --json
detmir-dlp
detmir-check --json
```

Перед возвратом DLP в production обязательно проверить ресурсный бюджет
Proxmox/InfluxDB/Grafana/ClickHouse. Не включайте DLP timers/services
автоматически вместе с обычным deploy, если текущая цель - сохранить лёгкий
Workforce/AW контур.

## Hayabusa/Velociraptor при выключенном DLP

Hayabusa/Sigma и Velociraptor относятся к optional security findings /
forensics layer, а не к тяжёлому DLP runtime:

- Hayabusa drop/autoprocess может продолжать работать при выключенном DLP;
- Velociraptor findings ingest может использоваться в offline/server mode, если
  администратор явно включил соответствующий режим;
- результаты должны попадать в Security Finding Inbox / ClickHouse как
  контролируемые findings, а не запускать автоматическую блокировку без
  approve/apply workflow;
- portal/workforce first screen не должен ждать Velociraptor или DLP;
- disabled DLP mode не должен превращаться в FAIL только из-за отсутствия DLP
  buckets.

Velociraptor server/client runtime не должен стартовать автоматически в
production DetMir без отдельного ресурсного решения. Для малой виртуальной
среды предпочтителен `disabled` или `offline_collector` режим.

## Ansible

В inventory/group vars:

```yaml
aw_dlp_profile: "core_only"
aw_dlp_enabled: false
aw_dlp_influx_enabled: false
aw_dlp_light_collector_enabled: false
aw_dlp_light_guard_enabled: true
detmir_portal_dlp_module_enabled_override: true
```

Для временного operator re-enable в `light`:

```yaml
aw_dlp_profile: "light"
aw_dlp_enabled: true
aw_dlp_influx_enabled: false
aw_dlp_light_collector_enabled: true
aw_dlp_light_guard_enabled: true
```

При `aw_dlp_profile: light` playbook включает только лёгкий агрегатор, IOC
refresh и load guard. DLP Influx exporter, report/syslog/webhook/CEF,
policy/case/evidence и Loki не должны возвращаться в active state.

## Ограничения

Это не удаление DLP-функциональности и не заявление, что DLP заменен другим средством. Это штатный режим временного/постоянного отключения тяжелого DLP runtime для стабилизации производительности. Исторические DLP buckets и артефакты могут оставаться на диске и в ActivityWatch до отдельной retention/cleanup процедуры.
