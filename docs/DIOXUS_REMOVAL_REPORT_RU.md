# Отчёт об удалении Dioxus из AWatch-rus

Дата: 2026-06-05

## Итог

Dioxus не используется в проекте и исключен из архитектурного roadmap.

Production-архитектура после проверки:

- Backend/API: Rust;
- Agent: Rust;
- Portal: Rust server-rendered HTML + HTMX;
- Future Enterprise UI: React + TypeScript через JSON API-контракты;
- Future Desktop Forensics: Tauri + React + Rust core.

## Удалённые и отсутствующие артефакты

В текущем дереве подтверждено отсутствие:

- workspace crate `adk-rust/crates/detmir-dpd-portal`;
- workspace member `crates/detmir-dpd-portal` в `adk-rust/Cargo.toml`;
- package entry `detmir-dpd-portal` в `adk-rust/Cargo.lock`;
- документа `docs/DPD_PORTAL_RU.md`;
- OpenAPI server base `/dpd/api`;
- исходного кода с `rsx!`;
- зависимостей `dioxus` или `dioxuslabs`.

В рамках финальной консолидации удалён устаревший частичный audit-документ
`docs/ARCHITECTURE_CLEANUP_AUDIT_2026-06-05_RU.md`; его содержимое заменено
этим отчётом.

## Зависимости

Дополнительные Dioxus-зависимости не удалялись, потому что в проверенном
workspace они уже отсутствуют. `adk-rust/Cargo.toml` не содержит Dioxus/DPD
members или workspace dependencies; поиск по `adk-rust` вне `target/` не
находит `dioxus`, `dioxuslabs`, `rsx!`, `detmir-dpd` или `dpd`.

## Проверенные маршруты

Контрактные и portal маршруты остаются в `adk-rust/crates/detmir-portal`:

- `/portal` - HTTP 200;
- `/reports` - HTTP 200;
- `/portal/reports` - HTTP 200;
- `/api/reports` - HTTP 200;
- `/api/contracts` - HTTP 200;
- `/api/contracts/openapi.json` - HTTP 200;
- `/api/contracts/typescript.d.ts` - HTTP 200.

`/dpd/` и `/dpd/api/*` не входят в публичный API-контракт и не публикуются в
OpenAPI.

## Выполненные проверки

Команды выполнены из `adk-rust`:

- `cargo fmt --check` - ok;
- `CARGO_TARGET_DIR=/tmp/aw-rus-cargo-target cargo clippy --all-targets --all-features -- -D warnings` - ok;
- `CARGO_TARGET_DIR=/tmp/aw-rus-cargo-target cargo test --all` - ok;
- `CARGO_TARGET_DIR=/tmp/aw-rus-cargo-target cargo build --all` - ok;
- `cargo tree --all-features` + поиск `dioxus|dioxuslabs|rsx!|detmir-dpd|dpd` - совпадений нет;
- `grep -RIn` по исходникам, документации и контрактам с исключением `.git`,
  `.ai`, `target`, `.local`, `dist`, `.ops`, `.playwright-cli` - совпадения
  только в этом отчёте;
- runtime smoke `detmir-portal` на `127.0.0.1:18720` - обязательные маршруты
  вернули HTTP 200;
- поиск по runtime JSON/OpenAPI/TypeScript ответам - `/dpd`, Dioxus и `rsx!`
  не найдены.

## Архитектурное подтверждение

Dioxus и связанная DPD-проба не являются production runtime, UI roadmap,
публичным API, workspace crate или dependency layer проекта. Дальнейшее
развитие портала выполняется через Rust SSR + HTML + HTMX и стабильные JSON
API-контракты для будущих React/Tauri клиентов.
