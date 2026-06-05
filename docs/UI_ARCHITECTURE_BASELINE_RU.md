# Архитектурный baseline UI AWatch-rus

Дата фиксации: 2026-06-05

## Baseline

```text
Backend/API: Rust
Agent: Rust
Current Portal: Rust server-rendered HTML + HTMX-ready static UI
Future Enterprise UI: React + TypeScript
Future Desktop Forensics: Tauri + React + Rust core
Dioxus: out of scope / не рассматривается
```

## Правила

- Текущий HTML/HTMX-ready портал остаётся основным pilot/production
  интерфейсом.
- JSON API является контрактным слоем для будущих UI.
- Бизнес-логика не должна зависеть от HTML.
- Будущий React/Tauri UI не должен ломать текущий портал.
- Agent и backend остаются Rust-first.
- Новые UI-фреймворки не добавлять без отдельного architecture decision.
- Dioxus не добавлять и не рассматривать.

## Практическое следствие

Развитие интерфейса выполняется в таком порядке:

1. Укрепить текущий Rust web portal.
2. Зафиксировать стабильные JSON API-контракты.
3. Поддерживать DPD `/dpd/` как параллельный mirror для проверки совместимости.
4. Готовить будущий React/Tauri UI только поверх опубликованных контрактов.

## Контрактный слой

Основные endpoints:

- `/api/contracts`;
- `/api/contracts/openapi.json`;
- `/api/contracts/typescript.d.ts`.

DPD mirror endpoints:

- `/dpd/api/contracts`;
- `/dpd/api/contracts/openapi.json`;
- `/dpd/api/contracts/typescript.d.ts`.

Правило совместимости: изменения API должны быть additive. Клиенты обязаны
игнорировать неизвестные поля и корректно обрабатывать отсутствующие optional
поля.
