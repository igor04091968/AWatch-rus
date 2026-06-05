# Аудит очистки архитектуры от DPD/Dioxus

Дата: 2026-06-05

## Цель

Зафиксировать результат проверки наследия DPD/Dioxus после усиления API
контрактов в коммите `bc2f56e`.

## Найденные DPD-артефакты до очистки

- workspace crate `adk-rust/crates/detmir-dpd-portal`;
- запись workspace member `crates/detmir-dpd-portal` в `adk-rust/Cargo.toml`;
- package entry `detmir-dpd-portal` в `adk-rust/Cargo.lock`;
- документ `docs/DPD_PORTAL_RU.md`;
- OpenAPI server base `/dpd/api`;
- упоминания DPD mirror в `docs/PORTAL_API_CONTRACTS_RU.md`;
- упоминания DPD mirror в `docs/UI_ARCHITECTURE_BASELINE_RU.md`.

## Найденные Dioxus-артефакты до очистки

Production-код, workspace crates, scripts, examples и OpenAPI/TypeScript
контракты не содержали Dioxus-зависимостей или `rsx!`-кода.

Оставались только запретительные упоминания в архитектурном baseline:

- Dioxus не является частью roadmap;
- Dioxus не добавлять без отдельного architecture decision.

## Выполненное решение

- DPD Portal исключён из workspace и публичного contract layer.
- Документ `docs/DPD_PORTAL_RU.md` удалён как описание экспериментальной ветки.
- OpenAPI больше не публикует `/dpd/api`.
- Runtime-слой DPD Portal выведен из эксплуатации: app-service остановлен и
  выключен, gateway route удалён, перед удалением создан локальный backup.
- `docs/ARCHITECTURE_BASELINE_RU.md`,
  `docs/UI_ARCHITECTURE_BASELINE_RU.md` и
  `docs/PORTAL_API_CONTRACTS_RU.md` фиксируют единственную целевую архитектуру:
  Rust backend/API, Rust agent, Rust SSR + HTML + HTMX portal, будущий
  React/TypeScript UI и будущий Tauri + React + Rust core desktop forensics.

## Статус

DPD/Dioxus остаются только как исторически упомянутые исключённые направления.
Они не являются production runtime, публичным API или частью roadmap.
