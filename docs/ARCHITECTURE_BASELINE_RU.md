# Архитектурный baseline AWatch-rus

Дата фиксации: 2026-06-05

## Целевая архитектура

```text
Backend/API: Rust
Agent: Rust
Portal: Rust SSR + HTML + HTMX
Future Enterprise UI: React + TypeScript
Future Desktop Forensics: Tauri + React + Rust core
```

## Поддерживаемый контур

- Текущий production/pilot портал: Rust server-rendered HTML с HTMX-ready
  поведением и обычным JavaScript без отдельного frontend framework.
- Контрактный слой для будущих интерфейсов: JSON API, OpenAPI 3.1 и
  TypeScript declarations.
- Бизнес-логика, расчёты, workflow и работа с JSON-хранилищами остаются на
  стороне Rust backend/API.
- Future Enterprise UI может быть реализован на React + TypeScript только
  поверх опубликованных JSON API.
- Future Desktop Forensics может быть реализован как Tauri + React + Rust core,
  не ломая текущий портал.

## Исключено из roadmap

Dioxus и DPD Portal исключены из архитектурного roadmap проекта.

Это означает:

- новые Dioxus/DPD crate не добавляются;
- `/dpd/` и `/dpd/api/*` не являются частью публичного API-контракта;
- будущий UI не должен парсить HTML текущего портала;
- новые UI-фреймворки требуют отдельного architecture decision;
- breaking changes API требуют version bump контракта и migration window.

## Compatibility baseline

Обязательные production endpoints:

- `/portal`;
- `/reports`;
- `/portal/reports`;
- `/api/reports`;
- `/api/contracts`;
- `/api/contracts/openapi.json`;
- `/api/contracts/typescript.d.ts`.

Правила совместимости:

- изменения API должны быть additive;
- клиенты игнорируют неизвестные поля;
- optional-поля могут отсутствовать;
- `null` не должен ломать UI;
- публичные JSON-поля не переименовываются без новой версии контракта.
