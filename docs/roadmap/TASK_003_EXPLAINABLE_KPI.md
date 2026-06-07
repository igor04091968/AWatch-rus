docs/roadmap/TASK_003_EXPLAINABLE_KPI.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: product feature / explainability / executive UX
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

Если есть smoke scripts — выполнить их.

---

Задача

Добавить explainability-слой для Workforce KPI:

Почему такой индекс активности?

---

Цель

Сделать индекс активности понятным для руководителя, безопасника и администратора без ML/LLM.

Система должна показывать не только итоговый KPI, но и объяснение:

- из чего он сложился;
- какие приложения внесли вклад;
- какие факторы снизили индекс;
- какие данные отсутствуют;
- насколько KPI надежен.

---

Контекст

AWatch-rus уже имеет Workforce-first направление:

- Executive Dashboard
- Workforce Portal
- Security Portal
- Forensics Portal
- Role-based доступ
- UEBA Score v1
- department comparison
- owner comparison
- trend status

Нужно усилить доверие к KPI, не добавляя ML.

---

Что реализовать

1. API explain endpoint

Добавить endpoint:

GET /api/workforce/kpi/explain

Поддержать параметры, если они уже соответствуют текущей архитектуре:

- date
- department
- owner
- employee_id, только если в проекте уже есть безопасная модель доступа
- role

Если конкретный employee-level доступ не готов — не добавлять его насильно.

---

2. Explainability model

Добавить структуру ответа:

{
  "kpi_score": 82,
  "confidence": "high",
  "coverage": {
    "agent_coverage_percent": 95,
    "data_freshness": "fresh",
    "missing_sources": []
  },
  "factors": [
    {
      "name": "productive_activity",
      "label": "Полезная активность",
      "impact": "+32",
      "explanation": "Высокая доля активности в рабочих приложениях"
    },
    {
      "name": "idle_time",
      "label": "Простой",
      "impact": "-8",
      "explanation": "Есть периоды неактивности в рабочее время"
    }
  ],
  "top_applications": [
    {
      "name": "1C",
      "category": "business",
      "active_minutes": 180,
      "contribution": "positive"
    }
  ],
  "warnings": [],
  "recommendations": [
    "Проверить сотрудников с низким покрытием данных"
  ]
}

---

3. Factors

Минимальные факторы:

- productive_activity
- business_app_usage
- idle_time
- afterhours_activity
- remote_session_activity
- data_coverage
- missing_data
- trend_change

Важно:

- факторы должны быть rule-based;
- не использовать ML;
- не использовать LLM;
- объяснение должно быть детерминированным;
- при нехватке данных возвращать "confidence: low".

---

4. Confidence

Добавить уровень доверия к KPI:

high
medium
low

Пример логики:

- high: хорошее покрытие, свежие данные, нет критичных пропусков;
- medium: есть частичные пропуски;
- low: мало данных или слабое покрытие.

---

5. UI в портале

Добавить секцию в Workforce/Executive portal:

Почему такой индекс активности?

Показать:

- итоговый KPI;
- confidence;
- coverage;
- положительные факторы;
- отрицательные факторы;
- top applications;
- warnings;
- recommendations.

Не перегружать интерфейс.
Стиль должен соответствовать существующему HTML/HTMX portal.

---

6. Role-based visibility

Соблюдать текущую ролевую модель:

Executive:

- агрегированный KPI;
- подразделения;
- тренды;
- без лишних персональных деталей.

Manager:

- свое подразделение;
- объяснение KPI по подразделению.

Security:

- только security-relevant факторы;
- не смешивать с HR-оценкой.

Forensics:

- детализация только для расследовательского контекста.

Admin:

- техническое покрытие и состояние источников.

---

7. Markdown report

Обновить markdown report.

Добавить раздел:

## Объяснение индекса активности

Включить:

- KPI score;
- confidence;
- coverage;
- основные положительные факторы;
- основные отрицательные факторы;
- warnings;
- рекомендации.

---

8. Tests

Добавить тесты:

- explain endpoint returns valid JSON;
- confidence high при хорошем покрытии;
- confidence low при нехватке данных;
- factors deterministic;
- role filtering не раскрывает лишнее;
- markdown report содержит explain section;
- existing reports не сломаны.

---

9. Документация

Добавить или обновить:

docs/EXPLAINABLE_KPI_RU.md

Описать:

- что такое explainable KPI;
- какие факторы используются;
- как считается confidence;
- что не является HR-оценкой;
- что не является ML/LLM;
- ограничения Pilot v1.

---

Запрещено

Не делать:

- ML
- LLM
- predictive scoring
- HR disciplinary scoring
- скрытые формулы без объяснения
- персональные выводы без role gate
- React
- Tauri
- Dioxus
- новую БД
- переписывание portal

---

Критерии приемки

Задача выполнена, если:

- добавлен explain API;
- добавлена explain model;
- portal показывает объяснение KPI;
- markdown report обновлен;
- role visibility соблюдена;
- документация добавлена;
- тесты проходят;
- существующий Pilot v1 не сломан.

---

Финальный отчет должен содержать

1. Краткое описание изменений.
2. Список измененных файлов.
3. API endpoint.
4. Описание модели explainability.
5. Добавленные UI-блоки.
6. Добавленные тесты.
7. Результаты проверок.
8. Известные ограничения.

---

## Выполнение

Статус: выполнено для Pilot v1.

Краткое описание:

- добавлен explainability-контракт Workforce KPI;
- добавлен endpoint `GET /api/workforce/kpi/explain`;
- добавлена детерминированная rule-based модель факторов;
- добавлен confidence level `high` / `medium` / `low`;
- UI показывает блок `Почему такой индекс активности?`;
- Markdown-отчет содержит explainability-раздел;
- OpenAPI и TypeScript contracts включают explain model;
- employee-level детализация не добавлена без отдельного безопасного контракта.

Ключевые файлы:

- `adk-rust/crates/detmir-portal/src/workforce_kpi_explain.rs`;
- `adk-rust/crates/detmir-portal/src/static/app.js`;
- `adk-rust/crates/detmir-portal/src/contracts/openapi.json`;
- `adk-rust/crates/detmir-portal/src/contracts/typescript.d.ts`;
- `docs/EXPLAINABLE_KPI_RU.md`.

API endpoint:

- `GET /api/workforce/kpi/explain`.

Модель explainability:

- `kpi_score`;
- `confidence`;
- `coverage`;
- `factors`;
- `top_applications`;
- `warnings`;
- `recommendations`.

Минимальные факторы:

- `productive_activity`;
- `business_app_usage`;
- `idle_time`;
- `afterhours_activity`;
- `remote_session_activity`;
- `data_coverage`;
- `missing_data`;
- `trend_change`.

Проверки:

- unit tests для explainability-модели и confidence;
- role-filtering smoke;
- markdown/report smoke;
- `cargo fmt --all --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- `cargo test --all`;
- `cargo build --release`;
- portal smoke.

Известные ограничения:

- не используется ML, LLM или predictive scoring;
- KPI не является HR-дисциплинарной оценкой;
- персональная explainability-модель в Pilot v1 не включена;
- качество объяснения зависит от свежести и полноты источников.
