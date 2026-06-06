# Архитектурный baseline UI AWatch-rus

Дата фиксации: 2026-06-05

## Baseline

```text
Backend/API: Rust
Agent: Rust
Current Portal: Rust server-rendered HTML + HTMX
Future Enterprise UI: React + TypeScript
Future Desktop Forensics: Tauri + React + Rust core
Experimental Rust UI/prototype mirrors: excluded from roadmap
```

## Правила

- Текущий HTML/HTMX портал остаётся основным pilot/production интерфейсом.
- JSON API является контрактным слоем для будущих UI.
- Бизнес-логика не должна зависеть от HTML.
- Будущий React/Tauri UI не должен ломать текущий портал.
- Agent и backend остаются Rust-first.
- Новые UI-фреймворки не добавлять без отдельного architecture decision.
- Экспериментальные Rust UI/prototype mirror направления не входят в
  архитектурный roadmap проекта.
- Будущий React/Tauri UI не должен парсить HTML текущего портала как источник
  данных.

## Практическое следствие

Развитие интерфейса выполняется в таком порядке:

1. Укрепить текущий Rust web portal.
2. Зафиксировать стабильные JSON API-контракты.
3. Готовить будущий React/Tauri UI только поверх опубликованных контрактов.
4. Любые breaking changes проводить только через version bump контракта и
   отдельное architecture decision.

## Контрактный слой

Основные endpoints:

- `/api/contracts`;
- `/api/contracts/openapi.json`;
- `/api/contracts/typescript.d.ts`.

Правило совместимости: изменения API должны быть additive. Клиенты обязаны
игнорировать неизвестные поля, корректно обрабатывать отсутствующие optional
поля и не падать на `null`.
