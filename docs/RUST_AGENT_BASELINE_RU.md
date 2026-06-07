# AWatch-rus Rust Agent Baseline

Документ описывает промышленный каркас будущего Rust Agent. Это baseline
агента, а не реализация мониторинга пользователя.

## Архитектура

Crate:

```text
adk-rust/crates/awatch-agent
```

Слои:

- `config` - загрузка конфигурации из файла и безопасные значения по умолчанию;
- `envelope` - единый telemetry envelope и heartbeat contract;
- `spool` - локальная очередь `spool/pending` и `spool/dead-letter`;
- `transport` - отправка envelope и retry/backoff;
- `health` - локальные `GET /healthz` и `GET /metrics`;
- `logging` - structured JSON logging;
- `metrics` - Prometheus text format.

Каркас не собирает активность пользователя и не реализует DLP/EDR-функции.

## Telemetry Envelope

Единый контракт:

```json
{
  "agent_id": "00000000-0000-5000-8000-000000000001",
  "host_id": "00000000-0000-5000-8000-000000000002",
  "platform": "windows",
  "timestamp": "2026-06-07T00:00:00Z",
  "records": []
}
```

`records` остается пустым или содержит только служебные записи baseline, пока
отдельные collectors не прошли acceptance gates.

## Heartbeat

Heartbeat записывается как служебный record:

```json
{
  "type": "heartbeat",
  "payload": {
    "agent_version": "0.1.0",
    "platform": "windows",
    "status": "online"
  }
}
```

Heartbeat не содержит hostname, список процессов, окна, clipboard, screenshots,
пакеты сети или содержимое документов.

## Local Spool

Очередь:

```text
spool/
├── pending/
└── dead-letter/
```

Поведение:

- `enqueue` пишет envelope атомарно через временный файл и rename;
- успешная отправка удаляет запись из `pending`;
- временная ошибка увеличивает `retry_count` и сохраняет запись;
- превышение `retry_max_attempts` переносит запись в `dead-letter`;
- поврежденный JSON переносится в `dead-letter` с reason-файлом.

## Retry

Retry использует bounded exponential backoff:

```text
base_backoff_ms * 2^attempt
```

Число попыток ограничено `retry_max_attempts`. Запись не удаляется из spool без
успешной отправки.

## Health

Локальный endpoint:

```text
GET /healthz
```

Ответ:

```json
{
  "ok": true,
  "status": "online",
  "agent_version": "0.1.0"
}
```

Endpoint предназначен для локального service/task health-check и не должен
публиковаться наружу без отдельного решения.

## Metrics

Prometheus metrics:

- `awatch_agent_queued_records`;
- `awatch_agent_retry_count`;
- `awatch_agent_heartbeat_sent`;
- `awatch_agent_spool_size`.

Локальный endpoint:

```text
GET /metrics
```

CLI-проверка:

```bash
awatch-agent --metrics
```

## Structured Logging

JSON log line содержит:

- `timestamp`;
- `level`;
- `agent_id`;
- `component`;
- `message`.

Пример:

```json
{
  "timestamp": "2026-06-07T00:00:00Z",
  "level": "INFO",
  "agent_id": "00000000-0000-5000-8000-000000000001",
  "component": "heartbeat",
  "message": "heartbeat envelope queued"
}
```

## CLI

Сформировать heartbeat envelope без отправки:

```bash
awatch-agent --print-envelope
```

Поставить heartbeat в локальную очередь:

```bash
awatch-agent --enqueue-heartbeat
```

Выгрузить spool:

```bash
awatch-agent --flush-spool
```

Запустить локальный health endpoint:

```bash
awatch-agent --healthz
```

## Ограничения

Запрещено и не реализовано:

- keylogger;
- screenshot capture;
- clipboard capture;
- packet interception;
- process injection;
- kernel drivers;
- EDR functionality;
- DLP functionality;
- ML;
- LLM.

Platform claims остаются ограниченными: baseline-каркас компилируется как Rust
crate, но production-поддержка конкретной ОС требует отдельной валидации.
