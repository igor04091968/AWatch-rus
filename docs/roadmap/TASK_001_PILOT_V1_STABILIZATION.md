# TASK 001: Pilot v1 Stabilization

## Цель

Закрепить Pilot v1 как стабильный демонстрационный и приемочный контур
AWatch-rus.

## Объем

- Проверка ролей `executive`, `manager`, `security`, `forensics`, `admin`.
- Проверка portal smoke и degraded-path smoke.
- Проверка документации Pilot v1 и runbook перед демонстрацией.
- Фиксация известных ограничений без расширения функциональности.

## Ограничения

- Не менять ролевую модель без отдельного решения.
- Не добавлять новые collectors.
- Не выдавать roadmap за реализованную функциональность.

## Результат

Pilot v1 можно показывать заказчику с понятным списком готовых возможностей,
ограничений и smoke-проверок.

---

## Выполнение

Статус: выполнено как часть Pilot v1 freeze.

Что закреплено:

- роли `executive`, `manager`, `security`, `forensics`, `admin`;
- серверные role gates для Pilot v1 API;
- Executive, Workforce, Security и Forensics portal views;
- Pilot v1 acceptance/evidence документация;
- demo/runbook слой для контролируемого показа;
- browser-level conformance smoke для ключевых представлений.

Ключевые артефакты:

- `docs/PILOT_V1_RU.md`;
- `docs/PILOT_V1_ACCEPTANCE_CHECKLIST_RU.md`;
- `docs/PILOT_V1_EVIDENCE_RU.md`;
- `docs/PILOT_VALIDATION_CHECKLIST_RU.md`;
- `docs/DEMO_RUNBOOK_RU.md`;
- `docs/BROWSER_CONFORMANCE_RU.md`;
- `scripts/detmir-portal-tabs-smoke.mjs`;
- `scripts/browser-conformance-smoke.mjs`;
- `scripts/pilot-validation-smoke.mjs`.

Проверки:

- `node scripts/pilot-validation-smoke.mjs`;
- `node scripts/detmir-portal-tabs-smoke.mjs` на локальном портале;
- `node scripts/browser-conformance-smoke.mjs` на локальном портале;
- `git diff --check`.

Известные ограничения:

- production acceptance требует отдельной live-проверки на стенде заказчика;
- screenshots из `artifacts/browser-smoke/` являются runtime artifacts и не
  коммитятся;
- новые collectors и новая функциональность в freeze-фазе не добавляются.
