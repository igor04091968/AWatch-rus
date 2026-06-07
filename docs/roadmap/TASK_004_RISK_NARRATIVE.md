
docs/roadmap/TASK_004_RISK_NARRATIVE.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: product feature / executive risk UX / reporting
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: minimal
Simplifications: forbidden
Security posture: no sensitive data exposure

Required checks:

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs

Задача

Добавить Risk Narrative слой для AWatch-rus.

Цель

Связать уже существующие данные:

- Workforce KPI;
- Explainable KPI;
- UEBA Score;
- Coverage;
- Risk Heatmap;
- Security Correlation;
- Incident Candidates;
- pfSense contract status;

в понятный управленческий вывод:

Что происходит?
Насколько это рискованно?
Почему система так считает?
Что делать дальше?

Контекст

AWatch-rus позиционируется как Workforce-first система с отдельными контурами:

- Executive;
- Workforce;
- Security;
- Forensics;
- Admin.

Нужно не добавлять новую SIEM/DLP/ML-систему, а собрать уже имеющиеся сигналы в объяснимый risk narrative.

Что реализовать

1. Risk Narrative model

Добавить модель:

{
  "risk_level": "medium",
  "risk_score": 62,
  "title": "Умеренный рост операционного риска",
  "summary": "Активность подразделения снизилась при росте удаленных сессий и частичных пробелах покрытия.",
  "why": [
    "Индекс активности ниже среднего по подразделениям",
    "UEBA score повышен до high",
    "Покрытие агентов ниже целевого уровня",
    "Есть кандидаты на инциденты"
  ],
  "evidence": [
    {
      "source": "workforce_kpi",
      "label": "Индекс активности",
      "value": "74%",
      "severity": "medium"
    },
    {
      "source": "ueba",
      "label": "UEBA score",
      "value": "high",
      "severity": "high"
    }
  ],
  "recommended_actions": [
    "Проверить подразделения с низким покрытием данных",
    "Проверить рост удаленных сессий",
    "Передать security-события в контур ИБ для анализа"
  ],
  "limitations": [
    "pfSense находится в contract_only режиме",
    "Risk Narrative не является ML-прогнозом"
  ]
}

2. API endpoint

Добавить endpoint:

GET /api/risk/narrative

Поддержать параметры, если они уже есть в текущей архитектуре:

- date;
- department;
- role;
- module.

Не добавлять employee-level детализацию, если нет готового безопасного контракта.

3. Rule-based risk scoring

Реализовать детерминированный rule-based scoring.

Пример уровней:

0-24   low
25-49  guarded
50-74  medium
75-89  high
90-100 critical

Сигналы для расчета:

- low Workforce KPI;
- low KPI confidence;
- low agent coverage;
- increased UEBA severity;
- incident candidates count;
- high security correlation;
- missing data;
- afterhours/remote activity;
- pfsense contract_only limitation.

Важно:

- не использовать ML;
- не использовать LLM;
- не использовать predictive scoring;
- все причины должны быть объяснимыми.

4. UI в Executive Portal

Добавить блок:

Риск-нарратив

Показать:

- уровень риска;
- risk score;
- краткое резюме;
- почему система так считает;
- evidence;
- recommended actions;
- limitations.

Интерфейс должен быть управленческим, не техническим.

5. UI в Security Portal

Добавить security-oriented view:

ИБ-интерпретация риска

Показать:

- security evidence;
- UEBA-related reasons;
- incident candidates;
- correlation indicators;
- что нужно проверить ИБ.

Не смешивать это с HR-оценкой сотрудников.

6. Markdown report

Обновить markdown report.

Добавить раздел:

## Риск-нарратив

Включить:

- risk level;
- risk score;
- summary;
- why;
- evidence;
- recommended actions;
- limitations.

7. OpenAPI / TypeScript contracts

Обновить:

- OpenAPI spec;
- TypeScript contracts;

если в проекте они уже поддерживаются.

8. Tests

Добавить тесты:

- "/api/risk/narrative" returns valid JSON;
- low-risk scenario;
- medium-risk scenario;
- high-risk scenario;
- limitations include contract_only where relevant;
- no ML/LLM claims;
- role visibility не раскрывает лишнее;
- markdown report contains risk narrative section;
- existing role gates not broken.

9. Documentation

Добавить:

docs/RISK_NARRATIVE_RU.md

Описать:

- что такое Risk Narrative;
- какие сигналы используются;
- как считается risk level;
- что означает evidence;
- что означает limitations;
- почему это не ML/LLM;
- почему это не SIEM и не DLP;
- как это показывать заказчику.

Запрещено

Не делать:

- ML;
- LLM;
- predictive analytics;
- полноценную SIEM;
- полноценную DLP;
- employee punishment scoring;
- новую БД;
- React;
- Tauri;
- Dioxus;
- SaaS-зависимости;
- ложные claims о pfSense ingestion.

Критерии приемки

Задача выполнена, если:

- добавлен "/api/risk/narrative";
- добавлена rule-based модель risk narrative;
- Executive UI показывает управленческий риск-нарратив;
- Security UI показывает ИБ-интерпретацию;
- markdown report обновлен;
- OpenAPI/TypeScript обновлены при необходимости;
- документация добавлена;
- тесты проходят;
- smoke проходит;
- существующие контракты не сломаны.

Финальный отчет Codex должен содержать

1. Краткое описание изменений.
2. Список измененных файлов.
3. Новый endpoint.
4. Risk scoring rules.
5. UI-блоки.
6. Обновления report/OpenAPI/TypeScript.
7. Добавленные тесты.
8. Результаты fmt/clippy/test/build/smoke.
9. Известные ограничения.

---

## Выполнение

Статус: выполнено для Pilot v1.

Краткое описание:

- добавлен rule-based Risk Narrative layer;
- добавлен endpoint `GET /api/risk/narrative`;
- risk score связывает Workforce KPI, Explainable KPI, UEBA, coverage,
  security correlation, incident candidates и pfSense `contract_only`
  limitation;
- Executive UI показывает блок `Риск-нарратив`;
- Security UI проверяет ИБ-релевантную связь рисков и активности;
- Markdown-отчет содержит раздел `## Риск-нарратив`;
- OpenAPI и TypeScript contracts обновлены;
- создана отдельная документация `docs/RISK_NARRATIVE_RU.md`.

Ключевые файлы:

- `adk-rust/crates/detmir-portal/src/risk_narrative.rs`;
- `adk-rust/crates/detmir-portal/src/static/app.js`;
- `adk-rust/crates/detmir-portal/src/contracts/openapi.json`;
- `adk-rust/crates/detmir-portal/src/contracts/typescript.d.ts`;
- `docs/RISK_NARRATIVE_RU.md`;
- `scripts/browser-conformance-smoke.mjs`;
- `scripts/detmir-portal-tabs-smoke.mjs`.

Risk scoring rules:

- `0-24` - `low`;
- `25-49` - `guarded`;
- `50-74` - `medium`;
- `75-89` - `high`;
- `90-100` - `critical`.

Сигналы:

- low Workforce KPI;
- low KPI confidence;
- low agent coverage;
- increased UEBA severity;
- incident candidates count;
- high security correlation;
- missing data;
- afterhours/remote activity;
- pfSense `contract_only` limitation.

UI-блоки:

- Executive: `Риск-нарратив`, `Почему`, `Подтверждения`, `Дальше`,
  `Ограничения`;
- Security: `Связь рисков и активности`, `Требует проверки`,
  `Рекомендуемые действия ИБ`;
- Forensics: расследования, timeline, материалы расследования и аудит.

Проверки:

- unit tests для risk narrative scenarios;
- OpenAPI/TypeScript contract smoke;
- `node scripts/browser-conformance-smoke.mjs`;
- `node scripts/detmir-portal-tabs-smoke.mjs`;
- `cargo fmt --all --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- `cargo test --all`;
- `cargo build --release`;
- `git diff --check`.

Известные ограничения:

- Risk Narrative не является ML/LLM/predictive analytics;
- Risk Narrative не подтверждает нарушение без ручной проверки;
- нет auto-remediation;
- pfSense не заявляется как ingestion/SIEM, пока это не пройдет отдельную
  приемку;
- live customer-stand validation остается отдельным шагом Demo Freeze v1.
