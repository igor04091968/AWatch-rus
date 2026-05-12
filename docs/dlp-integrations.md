# DLP Integrations (SIEM/SOAR)

## Что реализовано

- `aw-server/dlp-integrations/cef_exporter.py`  
  Читает новые события из `aw-dlp-incidents_*`, конвертирует в CEF и отправляет в syslog.
- `aw-server/dlp-integrations/webhook_sender.py`  
  Читает новые события из `aw-dlp-incidents_*` и отправляет webhook для нужных severity.
- Systemd:
  - `aw-dlp-cef-exporter.service` + `aw-dlp-cef-exporter.timer` (каждые 5 минут)
  - `aw-dlp-webhook-sender.service` + `aw-dlp-webhook-sender.timer` (каждые 2 минуты)

## Конфиги

- `/opt/activitywatch/dlp-integrations/cef-config.yaml`
- `/opt/activitywatch/dlp-integrations/webhook-config.yaml`

Поля:

- `aw_api_base` — URL AW API (`http://127.0.0.1:5600/api/0`)
- `state_path` — файл состояния last processed event id
- `per_bucket_limit` — лимит чтения событий из каждого `aw-dlp-incidents_*`

CEF:

- `syslog_host`, `syslog_port`, `syslog_proto` (`udp`/`tcp`)
- `severity_mapping` (`low/medium/high` -> CEF severity)

Webhook:

- `critical_webhooks` список:
  - `url`
  - `severity` (например `["high"]`)
- `retries`, `timeout_sec`, `backoff_base`

## Ansible

Интегрировано в:

- `ansible/deploy_aw_server.yml`
- `ansible/deploy_dlp_full_stack.yml`
- `ansible/roles/dlp-integrations/tasks/main.yml`

Флаг включения:

- `aw_dlp_integrations_enabled: true`
