 docs/roadmap/TASK_006_EXECUTIVE_ACTION_CENTER.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: executive decision support
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: minimal
Simplifications: forbidden

Required checks:

cargo fmt --all --check

cargo clippy --all-targets --all-features -- -D warnings

cargo test --all

cargo build --release

AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs

---

Цель

Преобразовать аналитику AWatch-rus в конкретные управленческие действия.

Система должна отвечать не только:

- что произошло;
- насколько это рискованно;

но и:

- что рекомендуется сделать;
- насколько это срочно;
- кому адресовано действие;
- почему система предлагает именно это действие.

---

Контекст

В проекте уже существуют:

- Workforce KPI
- Explainable KPI
- UEBA Score
- Risk Narrative
- Coverage
- Security Correlation
- Incident Candidates

Нужно использовать существующие сигналы.

Запрещено добавлять ML и LLM.

---

Что реализовать

1. Action Model

Добавить модель:

{
  "priority": "high",
  "title": "Проверить подразделение продаж",
  "summary": "Наблюдается снижение активности при росте удаленных сессий",
  "owner_role": "manager",
  "recommended_deadline": "24h",
  "reason_codes": [
    "LOW_WORKFORCE_KPI",
    "HIGH_REMOTE_ACTIVITY",
    "UEBA_HIGH"
  ],
  "evidence": [
    "Workforce KPI ниже среднего",
    "UEBA score повышен"
  ]
}

---

2. API

Добавить:

GET /api/actions

Добавить в:

/api/reports

секцию:

{
  "recommended_actions": []
}

---

3. Rule Engine

Сделать rule-based генерацию действий.

Примеры:

LOW_WORKFORCE_KPI

↓

Проверить подразделение

HIGH_UEBA

↓

Передать данные в контур ИБ

LOW_COVERAGE

↓

Проверить состояние агентов

INCIDENT_CANDIDATE

↓

Провести расследование

Все правила должны быть прозрачными и документированными.

---

4. Priority Levels

Поддержать:

low
medium
high
critical

---

5. Executive Portal

Добавить блок:

Рекомендуемые действия

Отображать:

- приоритет;
- описание;
- срок;
- обоснование.

---

6. Security Portal

Добавить:

Рекомендуемые действия ИБ

Отображать:

- security-related actions;
- incident actions;
- UEBA actions.

---

7. Markdown Report

Добавить раздел:

## Рекомендуемые действия

---

8. Documentation

Создать:

docs/EXECUTIVE_ACTION_CENTER_RU.md

Описать:

- механизм действий;
- уровни приоритета;
- правила генерации;
- ограничения.

---

9. Tests

Добавить тесты:

- action generation;
- priority assignment;
- role filtering;
- report generation;
- portal rendering;
- api response validation.

---

Запрещено

Не делать:

- ML;
- LLM;
- auto-remediation;
- автоматическую блокировку пользователей;
- автоматическое изменение политик;
- DLP;
- EDR;
- React;
- Tauri;
- Dioxus.

---

Критерии приемки

- /api/actions работает;
- Executive Portal показывает действия;
- Security Portal показывает действия;
- Markdown reports обновлены;
- документация добавлена;
- тесты проходят;
- smoke проходит;
- существующие контракты не сломаны.

---

Финальный отчет

1. Список файлов.
2. Новые endpoints.
3. Action model.
4. Rule engine.
5. UI изменения.
6. Документация.
7. Тесты.
8. Проверки.
9. Ограничения.

## Выполнение

Статус: done.

Файлы:

- `adk-rust/crates/detmir-portal/src/executive_actions.rs`;
- `adk-rust/crates/detmir-portal/src/main.rs`;
- `adk-rust/crates/detmir-portal/src/static/app.js`;
- `adk-rust/crates/detmir-portal/src/static/app.css`;
- `adk-rust/crates/detmir-portal/src/contracts/openapi.json`;
- `adk-rust/crates/detmir-portal/src/contracts/typescript.d.ts`;
- `adk-rust/crates/detmir-portal/src/production/limits.rs`;
- `scripts/awatch-production-hardening-smoke.mjs`;
- `docs/EXECUTIVE_ACTION_CENTER_RU.md`;
- `README.md`.

Новые endpoints:

- `GET /api/actions`.

Action model:

- `priority`;
- `title`;
- `summary`;
- `owner_role`;
- `recommended_deadline`;
- `reason_codes`;
- `evidence`.

Rule engine:

- deterministic rule-based;
- использует существующие сигналы Workforce KPI, UEBA, coverage, security
  correlation, incident candidates и Risk Narrative;
- не использует ML/LLM;
- не выполняет auto-remediation.

UI:

- Executive View показывает блок `Рекомендуемые действия`;
- Security View показывает блок `Рекомендуемые действия ИБ` с ИБ-действиями и
  действиями, передаваемыми в расследование;
- Markdown report содержит раздел `## Рекомендуемые действия`.

Ограничения:

- рекомендации не выполняются автоматически;
- пользователи не блокируются;
- политики не меняются;
- DLP/EDR/ML/LLM не добавлялись.
