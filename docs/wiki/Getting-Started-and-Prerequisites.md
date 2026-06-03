# Getting Started and Prerequisites

## 1.2 Обязательные переменные и preflight

После `cc9e4a0` контур Grafana/Influx считается частью базового production path. Перед запуском `ansible/deploy_aw_server.yml` должны быть доступны не только WinRM/SSH приватные параметры, но и write-token'ы InfluxDB для bucket `aw_metrics`.

Минимальный локальный файл приватной конфигурации:

```bash
set -a
source private-config/deploy.env
set +a
```

Обязательные переменные для Influx exporters:

| Переменная | Назначение |
| --- | --- |
| `AW_WORKTIME_INFLUX_TOKEN` | Write-token для `aw-worktime-influx-exporter.service`; пишет `aw_rdp_worktime_*` ряды в `aw_metrics`. |
| `AW_DLP_INFLUX_TOKEN` | Write-token для `aw-dlp-influx-exporter.service`; пишет DLP health/signals/cases/reviews/rules в `aw_metrics`. |

Токены должны иметь право записи в bucket `aw_metrics` в org `proxmox`. Read-only token из Grafana datasource не подходит: exporter получит `HTTP 403 Forbidden`.

## Проверки перед deploy

Быстрый preflight:

```bash
test -n "$AW_WORKTIME_INFLUX_TOKEN"
test -n "$AW_DLP_INFLUX_TOKEN"
ansible-playbook --syntax-check ansible/deploy_aw_server.yml -i ansible/inventory.ini
ansible-playbook --syntax-check ansible/deploy_aw_windows.yml -i ansible/inventory.ini
```

`deploy_aw_server.yml` теперь сам валидирует token'ы: если `aw_worktime_influx_enabled=true` или `aw_dlp_influx_enabled=true`, но соответствующий token пустой, playbook останавливается до записи `/etc/activitywatch/aw-server.env`.

## Runtime smoke-check

После deploy проверьте:

```bash
systemctl is-active aw-worktime-influx-exporter.timer
systemctl is-active aw-dlp-influx-exporter.timer
journalctl -u aw-worktime-influx-exporter.service -n 20 --no-pager
journalctl -u aw-dlp-influx-exporter.service -n 20 --no-pager
```

Ожидаемый результат: oneshot services завершаются `status=0/SUCCESS`, в журнале есть строки вида `wrote ... points to aw_metrics`.
