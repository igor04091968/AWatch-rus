# AWatch-rus Agent Deployment

## Конфигурация

Пример:

```toml
server_url = "https://awatch.local/api/telemetry"
api_key = "change-me"
collect_interval_seconds = 60
role = "workstation"

enable_processes = true
enable_network = true
enable_security_events = true
enable_workforce_activity = true

spool_dir = "/var/lib/awatch-agent/spool"
timeout_seconds = 10
retry_attempts = 3
```

Linux/BSD путь по умолчанию:

```text
/etc/awatch-agent/awatch-agent.toml
```

Windows путь по умолчанию:

```text
C:\ProgramData\AWatch\agent\awatch-agent.toml
```

## Запуск

Однократный сбор и печать JSON:

```bash
awatch-agent-rs --once --print-json
```

Однократная отправка:

```bash
awatch-agent-rs --once --config /etc/awatch-agent/awatch-agent.toml
```

Проверка очереди:

```bash
awatch-agent-rs --spool-health
```

Отправка накопленной очереди:

```bash
awatch-agent-rs --flush-spool
```

## Server API

Портал принимает телеметрию через:

```text
POST /api/telemetry
```

Требования:

- JSON object;
- заголовок `x-api-key` или `Authorization: Bearer`;
- обязательные поля `TelemetryRecord`;
- в v0.3 хранение является prototype file-backed JSONL.

Серверные переменные:

```text
DETMIR_PORTAL_TELEMETRY_API_KEY
DETMIR_PORTAL_TELEMETRY_STORE_PATH
```

По умолчанию `change-me` не авторизует прием телеметрии. Для пилота ключ нужно задать явно.

## Проверка после установки

1. Запустить агент с `--once --print-json`.
2. Проверить наличие `TelemetryRecord` и `collector_version`.
3. Запустить агент с реальным `server_url`.
4. Проверить HTTP-ответ `{ "ok": true, "stored": "file-backed-jsonl" }`.
5. Проверить отсутствие файлов в spool после успешной отправки.
