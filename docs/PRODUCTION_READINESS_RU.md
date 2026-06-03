# Контроль готовности промышленного внедрения

`detmir-readiness` - единая команда preflight-контроля перед внедрением,
релизом или изменением production runtime.

Команда проверяет:

- runtime env без public placeholders;
- активность обязательных systemd units;
- реальную запись в InfluxDB;
- health Grafana datasource.

## Базовый запуск

На AW server:

```bash
detmir-readiness --json
```

Ожидаемый результат:

```json
{
  "ok": true,
  "status": "OK"
}
```

Коды возврата:

- `0` - готово к промышленной эксплуатации;
- `2` - readiness check нашел `WARN` или `FAIL`;
- `1` - сама команда не смогла выполниться.

## Private production inventory

Перед rollout private override-файлы проверяются отдельно:

```bash
scripts/check_production_inventory_placeholders.sh --strict \
  private-config/runtime.env \
  private-config/ansible-vars.yml
```

`--strict` предназначен только для private production-файлов. Публичные
tracked defaults и `.example` файлы могут содержать `HOST-EXAMPLE` и TEST-NET
адреса, потому что они не являются production source of truth.

## Что считается отказом

`detmir-readiness` возвращает `FAIL`, если:

- включенный Influx exporter получил пустой или example URL/org/bucket/token/host;
- systemd unit из обязательного списка не active;
- Influx write-probe не смог записать heartbeat;
- Grafana datasource health не `OK`.

## Полезные параметры

```bash
detmir-readiness --json \
  --aw-env-file /etc/activitywatch/aw-server.env \
  --grafana-env-file /etc/detmir-grafana-check.env \
  --grafana-datasource-uid influxdb_aw
```

Для диагностики без write-probe:

```bash
detmir-readiness --json --skip-influx-write
```

Для контура, где Influx временно не входит в профиль внедрения:

```bash
detmir-readiness --json --allow-disabled-influx
```

Такой запуск допустим только как временный исключительный режим; для полного
commercial DetMir contour Influx/Grafana должны быть зелеными.
