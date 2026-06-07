.docs/roadmap/TASK_005_RUST_AGENT_BASELINE.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: agent architecture
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: minimal
Simplifications: forbidden

Цель

Подготовить промышленную основу будущего Rust Agent без внедрения реального мониторинга пользователя.

Задача не про сбор данных.

Задача про создание правильного каркаса агента.

Контекст

Текущий курс проекта:

Backend/API:
Rust

Portal:
HTML + HTMX

Future Enterprise UI:
React + TypeScript

Agent:
Rust

PowerShell не является целевой архитектурой.

Что реализовать

Создать отдельный agent crate.

Пример:

adk-rust/crates/awatch-agent/

Добавить:

- config loader;
- local spool;
- retry queue;
- telemetry envelope;
- heartbeat contract;
- health endpoint;
- graceful shutdown;
- structured logging;
- metrics.

Telemetry Envelope

Создать единый контракт:

{
  "agent_id": "uuid",
  "host_id": "uuid",
  "platform": "windows",
  "timestamp": "...",
  "records": []
}

Без реального сбора активности.

Только каркас.

Local Spool

Реализовать:

spool/

Возможности:

- enqueue;
- dequeue;
- retry;
- corruption detection.

Retry Logic

Добавить:

- exponential backoff;
- max retry count;
- dead letter state.

Heartbeat

Добавить:

{
  "agent_version": "...",
  "platform": "...",
  "status": "online"
}

Без реального inventory.

Health

Добавить локальный:

GET /healthz

Для самого агента.

Logging

Structured JSON logging.

Минимальные поля:

- timestamp;
- level;
- agent_id;
- component;
- message.

Metrics

Prometheus metrics для агента:

- queued_records;
- retry_count;
- heartbeat_sent;
- spool_size.

Documentation

Добавить:

docs/RUST_AGENT_BASELINE_RU.md

Описать:

- архитектуру агента;
- spool;
- retry;
- heartbeat;
- telemetry envelope.

Запрещено

Не делать:

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

Критерии приемки

- новый crate создан;
- сборка проходит;
- тесты проходят;
- spool работает;
- retry работает;
- heartbeat работает;
- документация добавлена.

Финальный отчет Codex

1. Список файлов.
2. Архитектура агента.
3. Контракт telemetry envelope.
4. Реализация spool.
5. Реализация retry.
6. Реализация heartbeat.
7. Результаты проверок.
8. Известные ограничения.

