docs/roadmap/TASK_003A_PORTAL_HARDENING_CLEANUP.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: refactoring / maintainability / regression safety
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: forbidden
Simplifications: forbidden

Required checks:

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs

Задача

Провести безопасный cleanup после коммита production hardening + explainable KPI.

Цель

Снизить размер и сложность "adk-rust/crates/detmir-portal/src/main.rs", не меняя поведение портала и API.

Что сделать

1. Вынести production hardening код в отдельные модули:

src/production/
├── health.rs
├── readiness.rs
├── version.rs
├── metrics.rs
├── limits.rs
├── request_context.rs
└── logging.rs

2. Вынести explainable KPI код в отдельный модуль:

src/workforce_kpi_explain.rs

3. Сохранить публичное поведение endpoints:

- GET /healthz
- GET /readyz
- GET /version
- GET /metrics
- GET /api/workforce/kpi/explain

4. Не менять JSON-контракты без необходимости.

5. Не менять OpenAPI/TypeScript contracts, если поведение не изменилось.

6. Добавить unit tests там, где логика стала модульной:

- config validation;
- query limits;
- request id / correlation id;
- KPI confidence;
- deterministic KPI factors.

7. Smoke script не ломать.

Запрещено

Не добавлять:

- новые функции;
- React;
- Tauri;
- Dioxus;
- ML/LLM;
- новую БД;
- новые внешние зависимости без крайней необходимости.

Критерии приемки

- main.rs стал меньше и чище;
- endpoints работают как раньше;
- smoke проходит;
- contracts не сломаны;
- role gates не сломаны;
- документация не ухудшена;
- все проверки проходят.

Финальный отчет Codex

В отчете указать:

1. Какие модули созданы.
2. Что вынесено из main.rs.
3. Изменился ли внешний API.
4. Какие тесты добавлены.
5. Результаты fmt/clippy/test/build/smoke.