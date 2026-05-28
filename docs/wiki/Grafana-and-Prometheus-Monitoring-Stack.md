# Grafana and Prometheus Monitoring Stack

## 7. Grafana / Influx exporters

После `cc9e4a0` Influx exporters для Grafana включены как production default:

```yaml
aw_worktime_influx_enabled: true
aw_dlp_influx_enabled: true
```

Оба exporter'а пишут в InfluxDB bucket:

```text
org: proxmox
bucket: aw_metrics
url: http://10.10.10.10:8086
```

Grafana datasource `InfluxDB-AW` читает тот же bucket.

## Token parameters

Новые обязательные параметры:

| Ansible var | Env source | Назначение |
| --- | --- | --- |
| `aw_worktime_influx_token` | `AW_WORKTIME_INFLUX_TOKEN` | Write-token для `aw-worktime-influx-exporter.service`. |
| `aw_dlp_influx_token` | `AW_DLP_INFLUX_TOKEN` | Write-token для `aw-dlp-influx-exporter.service`. |

Эти token'ы должны иметь `write-bucket` permission на `aw_metrics`. Grafana read-token не подходит для exporters.

## Deploy validation

`deploy_aw_server.yml` теперь содержит preflight assert'ы:

- если `aw_worktime_influx_enabled=true`, `aw_worktime_influx_token` обязан быть непустым;
- если `aw_dlp_influx_enabled=true`, `aw_dlp_influx_token` обязан быть непустым.

Кроме того, разовый запуск exporters больше не маскируется `failed_when: false`. Если запись в Influx сломана, playbook должен явно упасть, а не оставлять Grafana со старыми рядами.

## Expected measurements

После успешного запуска в `aw_metrics` должны быть свежие ряды:

- `aw_window_event`
- `aw_afk_event`
- `aw_rdp_worktime_daily`
- `aw_rdp_worktime_hourly`
- `aw_rdp_worktime_summary_daily`
- `aw_dlp_endpoint_self_test`
- `aw_dlp_fileops_health`
- `aw_dlp_signal`
- DLP case/review/rule/incident measurements, если в источниках есть соответствующие события.

Быстрая диагностика:

```bash
systemctl start aw-worktime-influx-exporter.service
systemctl start aw-dlp-influx-exporter.service
journalctl -u aw-worktime-influx-exporter.service -n 30 --no-pager
journalctl -u aw-dlp-influx-exporter.service -n 30 --no-pager
```

Успешный результат: `wrote ... points to aw_metrics`.

## Grafana checks

Через API:

```bash
curl -u "$GRAFANA_USER:$GRAFANA_PASSWORD" \
  http://10.10.10.11:3000/api/datasources/uid/influxdb_aw/health
```

Ожидается:

```json
{"message":"datasource is working. 1 buckets found","status":"OK"}
```

Dashboard UID, которые должны открываться:

- `detmir-aw-main`
- `detmir-rdp-user-activity`
- `detmir-dlp-security`
- `detmir-dlp-management`
- `awatch-dlp-overview`
