docs/roadmap/TASK_002_PRODUCTION_HARDENING.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: production hardening / reliability / observability
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: minimal
Simplifications: forbidden
Security posture: fail closed

Required checks:

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release

Если есть smoke scripts — выполнить их.
Если smoke scripts отсутствуют — добавить минимальный smoke script.

---

Задача

Усилить production-ready слой AWatch-rus после Pilot v1 без изменения основной архитектуры.

---

Цель

Добавить:

- "/healthz"
- "/readyz"
- "/version"
- "/metrics"
- строгую валидацию конфигурации
- request id / correlation id
- structured JSON logging
- Prometheus metrics
- request timeout / payload limits / page limits
- защиту от тяжелых API-запросов
- smoke-тесты
- документацию

---

Контекст проекта

AWatch-rus — Workforce-first система с модулями:

- Executive
- Workforce
- Security
- Forensics
- Admin

Текущий стек:

- Rust backend
- Rust server-rendered HTML + HTMX portal
- API contracts
- Dioxus не используется
- React/Tauri не трогать
- ML/LLM не добавлять

---

Что реализовать

1. Health endpoint

Добавить:

GET /healthz

Поведение:

- отвечает "200 OK", если процесс жив;
- не проверяет внешние зависимости;
- ответ JSON.

Пример:

{
  "status": "ok"
}

---

2. Readiness endpoint

Добавить:

GET /readyz

Поведение:

- отвечает "200 OK", если приложение готово обслуживать запросы;
- проверяет только реально существующие зависимости;
- если зависимостей нет — явно вернуть local-ready состояние;
- не делать ложных claim о pfSense ingestion, SIEM, DLP или внешних интеграциях.

Пример:

{
  "status": "ready",
  "checks": {
    "config": "ok",
    "storage": "not_configured",
    "pfsense": "contract_only"
  }
}

При ошибке готовности:

- вернуть "503";
- JSON должен объяснять причину.

---

3. Version endpoint

Добавить:

GET /version

Ответ должен включать:

{
  "app_version": "0.2.0",
  "git_commit": "unknown",
  "build_time": "unknown",
  "schema_version": "pilot-v1",
  "environment": "local"
}

Требования:

- если git commit/build time недоступны — не падать;
- не раскрывать секреты;
- использовать безопасные значения по умолчанию.

---

4. Metrics endpoint

Добавить:

GET /metrics

Формат:

Prometheus text format

Минимальные метрики:

- "awatch_http_requests_total"
- "awatch_http_request_duration_seconds"
- "awatch_reports_generated_total"
- "awatch_ingestion_records_total"
- "awatch_ingestion_rejected_total"
- "awatch_role_denied_total"
- "awatch_readyz_status"

Labels:

- "method"
- "route"
- "status"
- "module"

Запрещено использовать high-cardinality labels:

- user_id
- employee_id
- ip
- raw URL
- query params

---

5. Валидация конфигурации

Добавить централизованную проверку конфигурации при старте.

Проверить:

- host
- port
- max page size
- default page size
- max report date range
- request timeout
- payload size limit
- environment name
- enabled modules

Поведение:

- при невалидной конфигурации приложение должно завершиться с понятной ошибкой;
- секреты не логировать;
- небезопасные значения не подставлять молча.

Добавить unit tests:

- valid config
- invalid port
- invalid page size
- invalid report range
- invalid timeout

---

6. Request ID / Correlation ID

Добавить middleware:

- принимать входящий "X-Request-Id";
- если его нет — генерировать новый;
- принимать входящий "X-Correlation-Id";
- если его нет — использовать request_id;
- возвращать оба header в response;
- добавлять оба значения в logs.

Headers:

X-Request-Id
X-Correlation-Id

---

7. Structured JSON logging

HTTP request logs должны включать:

- timestamp
- level
- request_id
- correlation_id
- method
- path
- status
- latency_ms
- user_role, если известна
- error_code, если есть

Запрещено логировать:

- токены
- секреты
- полные body payload
- персональные данные

---

8. Timeouts and limits

Добавить защитные лимиты:

- max request body size
- max report date range
- max page size
- default page size
- request timeout
- slow request logging threshold

Поведение:

- слишком большой body → "413"
- слишком большой page_size → "400"
- слишком широкий report range → "400"
- timeout → "408" или "504", согласно текущей архитектуре

---

9. Защита тяжелых API-запросов

Проверить и защитить:

- "/api/reports"
- "/api/executive"
- "/api/workforce"
- "/api/security"
- "/api/forensics"
- "/api/ueba"
- "/api/pfsense"

Требования:

- не должно быть неограниченных выборок;
- date range должен иметь максимум;
- page size должен иметь максимум;
- ошибки должны быть понятными;
- role gates не должны быть сломаны.

---

10. Документация

Добавить:

docs/PRODUCTION_READINESS_RU.md

Описать:

- "/healthz"
- "/readyz"
- "/version"
- "/metrics"
- конфигурацию
- лимиты
- формат логов
- список метрик
- smoke checks
- что является "contract_only"

---

11. Smoke tests

Smoke должен проверять:

- "/healthz" возвращает 200
- "/readyz" возвращает 200 или ожидаемый 503 с JSON
- "/version" содержит "app_version" и "schema_version"
- "/metrics" возвращает Prometheus text
- "X-Request-Id" возвращается в headers
- слишком большой "page_size" отклоняется
- слишком широкий report range отклоняется
- role gates работают

---

Запрещено

Не делать:

- Dioxus
- React
- Tauri
- ML
- LLM
- новую БД
- SaaS-зависимости
- ложные заявления о pfSense/SIEM/DLP ingestion
- переписывание архитектуры
- удаление существующих API
- поломку HTML/HTMX portal

---

Критерии приемки

Задача считается выполненной, если:

- endpoints добавлены;
- лимиты работают;
- конфиг валидируется;
- metrics доступны;
- request id/correlation id работают;
- structured logs работают;
- heavy API requests ограничены;
- документация добавлена;
- все проверки проходят;
- существующий Pilot v1 функционал не сломан.

---

Финальный отчет должен содержать

1. Краткое описание изменений.
2. Список измененных файлов.
3. Добавленные endpoints.
4. Добавленные лимиты.
5. Добавленные метрики.
6. Добавленные тесты.
7. Результаты команд проверки.
8. Результат smoke.
9. Известные ограничения

---

## Выполнение

Статус: выполнено для Pilot v1 production-hardening слоя портала.

Краткое описание:

- добавлены production endpoints `/healthz`, `/readyz`, `/version`, `/metrics`;
- добавлены request id / correlation id headers;
- добавлены structured JSON HTTP logs;
- добавлены bounded query/body limits для тяжелых API;
- добавлена валидация production-конфигурации;
- role gates сохранены и проверяются smoke;
- pfSense остается `contract_only`, без заявления ingestion/SIEM.

Ключевые файлы:

- `adk-rust/crates/detmir-portal/src/production/`;
- `adk-rust/crates/detmir-portal/src/main.rs`;
- `docs/PRODUCTION_READINESS_RU.md`;
- `scripts/awatch-production-hardening-smoke.mjs`.

Endpoints:

- `GET /healthz`;
- `GET /readyz`;
- `GET /version`;
- `GET /metrics`;
- защищенные Pilot v1 API: `/api/reports`, `/api/executive`,
  `/api/workforce`, `/api/security`, `/api/forensics`, `/api/ueba`,
  `/api/pfsense`, `/api/workforce/kpi/explain`.

Лимиты и защита:

- max request body size;
- max/default page size;
- max report date range;
- request timeout и slow request logging threshold;
- отказ `400` для слишком большого `page_size` или диапазона отчета;
- отказ `413` для слишком большого body;
- отказ `403` по role gate.

Метрики:

- `awatch_http_requests_total`;
- `awatch_http_request_duration_seconds`;
- `awatch_reports_generated_total`;
- `awatch_ingestion_records_total`;
- `awatch_ingestion_rejected_total`;
- `awatch_role_denied_total`;
- `awatch_readyz_status`.

Проверки:

- `cargo fmt --all --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- `cargo test --all`;
- `cargo build --release`;
- `AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs`;
- `git diff --check`.

Известные ограничения:

- production-hardening smoke не заменяет live acceptance;
- `/readyz` отражает только реально настроенные зависимости;
- contract-only интеграции не считаются работающими сборщиками;
- freeze-фаза допускает только исправление дефектов и уточнение документации.
