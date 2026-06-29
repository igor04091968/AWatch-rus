# Optional DLP runtime for DetMir

Цель: DLP-контур должен отключаться управляемо, без ложных аварий в health/readiness и без автоматического подъема heavy-пайплайна, когда задача контура - снизить нагрузку на InfluxDB, Grafana и ClickHouse.

## Что отключается

Штатный runtime off включает:

- `AW_DLP_ENABLED=false` на AW server;
- `DETMIR_DLP_ENABLED=false` в управляющем DetMir contour check;
- `DETMIR_PORTAL_DLP_MODULE_ENABLED=false` для portal UI/API DLP-модуля;
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

Worktime, ActivityWatch server, browser/window/AFK collection, Hayabusa and 1C/ClickHouse core не считаются DLP runtime и отдельно не отключаются.

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

Отдельные non-DLP findings того же ручного прогона:

- AFK/window/worktime buckets were stale and require RDP collector/session
  recovery;
- WinRM from server side to `192.168.100.18:5985` was unreachable;
- these findings are outside the DLP runtime disable boundary.

## Возврат DLP

```bash
sudo sed -i 's/^AW_DLP_ENABLED=.*/AW_DLP_ENABLED=true/' /etc/activitywatch/aw-server.env
sudo /usr/local/bin/detmir-dlp-runtime-control enable
sudo systemctl restart aw-worktime-api.service || true
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

## Ansible

В inventory/group vars:

```yaml
aw_dlp_enabled: false
aw_dlp_disabled_reason: "operator_disabled_to_reduce_influx_grafana_clickhouse_load"
aw_dlp_disabled_since: "2026-06-25"
detmir_portal_dlp_module_enabled_override: false
```

При `aw_dlp_enabled: false` playbook пишет `AW_DLP_ENABLED=false`, не включает DLP service/timer runtime и не должен возвращать DLP Influx exporter/aggregator в active state.

## Ограничения

Это не удаление DLP-функциональности и не заявление, что DLP заменен другим средством. Это штатный режим временного/постоянного отключения тяжелого DLP runtime для стабилизации производительности. Исторические DLP buckets и артефакты могут оставаться на диске и в ActivityWatch до отдельной retention/cleanup процедуры.
