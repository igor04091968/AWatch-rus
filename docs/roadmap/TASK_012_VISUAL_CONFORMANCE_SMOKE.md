# docs/roadmap/

## Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: visual validation
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: forbidden
New features: forbidden

## Цель

Добавить автоматическую проверку того, что ключевые элементы портала реально отображаются в браузере.

Не добавлять новый функционал.

Проверять существующий.

---

## Контекст

После TASK_011 выявлен gap:

Отсутствует browser-level conformance validation.

Есть API smoke.

Есть docs smoke.

Есть deployment smoke.

Но нет проверки визуального отображения ключевых блоков.

---

## Что реализовать

Создать browser smoke.

Рекомендуется:

* Playwright

или

* существующий инструмент проекта, если он уже есть.

---

## Проверяемые страницы

Проверить наличие:

### Executive

* KPI
* Explainable KPI
* Risk Narrative
* Recommended Actions

### Workforce

* KPI
* Department Comparison
* Trend Status

### Security

* UEBA
* Risk Narrative
* Recommended Actions

### Forensics

* Reports
* Investigation data, если уже есть

---

## Проверяемые элементы

Не проверять дизайн.

Проверять:

* наличие блоков;
* наличие заголовков;
* отсутствие ошибки рендера;
* отсутствие пустой страницы;
* отсутствие 500.

---

## Screenshots

Сохранять screenshots:

```text
artifacts/browser-smoke/
```

Минимум:

* executive.png
* workforce.png
* security.png
* forensics.png

---

## Smoke Script

Создать:

```text
scripts/browser-conformance-smoke.*
```

---

## Documentation

Создать:

```text
docs/BROWSER_CONFORMANCE_RU.md
```

Описать:

* что проверяется;
* ограничения;
* как запускать.

---

## Запрещено

Не делать:

* новые API;
* новые UI;
* новые данные;
* новые claims.

---

## Критерии приемки

* browser smoke существует;
* screenshots генерируются;
* smoke проходит локально;
* документация добавлена.

---

## Финальный отчет

1. Используемый инструмент.
2. Проверяемые страницы.
3. Проверяемые блоки.
4. Примеры screenshots.
5. Результаты smoke.
6. Ограничения.

---

## Выполнение

Статус: выполнено.

Используемый инструмент:

- Playwright через существующий локальный loader проекта.

Создан smoke:

- `scripts/browser-conformance-smoke.mjs`.

Создана документация:

- `docs/BROWSER_CONFORMANCE_RU.md`.

Обновлен README:

- добавлена ссылка на browser conformance smoke.

Проверяемые страницы/роли:

- Executive / `data-view-mode="executive"`;
- Workforce / `data-view-mode="manager"`;
- Security / `data-view-mode="security"`;
- Forensics / `data-view-mode="forensics"`.

Проверяемые блоки:

- KPI;
- Explainable KPI;
- Risk Narrative;
- Recommended Actions;
- Department Comparison;
- Trend Status;
- события безопасности;
- связь рисков и активности;
- Investigation data;
- timeline событий;
- материалы расследования;
- аудит.

Примечание: отдельный блок Risk Narrative проверяется в Executive view. В
Security view smoke проверяет уже существующий блок связи рисков и активности,
не добавляя новый UI и не создавая новый claim.

Screenshots:

- `artifacts/browser-smoke/executive.png`;
- `artifacts/browser-smoke/workforce.png`;
- `artifacts/browser-smoke/security.png`;
- `artifacts/browser-smoke/forensics.png`.

Ограничения:

- smoke не проверяет дизайн pixel-perfect;
- smoke не добавляет новые API, UI или данные;
- screenshots являются runtime artifacts и не коммитятся в Git;
- live customer-stand visual validation остается отдельным acceptance шагом.
