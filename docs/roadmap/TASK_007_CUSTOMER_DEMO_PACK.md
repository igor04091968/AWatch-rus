
docs/roadmap/TASK_007_CUSTOMER_DEMO_PACK.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: customer demo readiness
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: forbidden
Simplifications: forbidden

Цель

Подготовить полноценный демонстрационный пакет для пилотного показа заказчику.

Не добавлять новую функциональность.

Использовать уже реализованные возможности.

---

Что реализовать

1. Demo Dataset

Создать демонстрационный набор данных.

Требования:

- только синтетические данные;
- без реальных сотрудников;
- без реальных IP;
- без реальных доменов;
- без реальных логинов;
- без персональных данных.

Покрыть сценарии:

- нормальная работа;
- снижение активности;
- рост удаленных сессий;
- повышенный UEBA;
- incident candidate;
- низкое покрытие агентами.

---

2. Demo Scenario Pack

Создать:

docs/demo/

Файлы:

DEMO_SCENARIO_EXECUTIVE_RU.md
DEMO_SCENARIO_SECURITY_RU.md
DEMO_SCENARIO_FORENSICS_RU.md

Каждый сценарий должен содержать:

- что показывать;
- в каком порядке;
- какие выводы делать;
- какие вопросы ожидать от заказчика.

---

3. Executive Demo Flow

Подготовить сценарий:

Руководитель
↓
KPI
↓
Risk Narrative
↓
Recommended Actions

Цель:

показать управленческую ценность.

---

4. Security Demo Flow

Подготовить сценарий:

UEBA
↓
Incident Candidates
↓
Risk Narrative
↓
Recommended Actions

Цель:

показать пользу для ИБ.

---

5. Forensics Demo Flow

Подготовить сценарий:

Событие
↓
Контекст
↓
Evidence
↓
Отчет

Использовать только реально существующие возможности проекта.

Не придумывать функционал.

---

6. Demo Screenshots

Проверить актуальность всех PNG.

Если нужно:

обновить screenshots.

Только демонстрационные данные.

---

7. Demo Report

Добавить:

docs/DEMO_REPORT_EXAMPLE_RU.md

Показать пример итогового отчета:

- KPI;
- Explainable KPI;
- Risk Narrative;
- Recommended Actions.

---

8. Pilot Value Statement

Добавить:

docs/PILOT_VALUE_PROPOSITION_RU.md

Структура:

- проблема заказчика;
- что решает AWatch-rus;
- выгоды для руководителя;
- выгоды для ИБ;
- выгоды для расследований;
- ограничения пилота.

---

9. Smoke

Проверить:

- demo dataset загружается;
- screenshots существуют;
- ссылки в документации валидны.

---

Запрещено

Не делать:

- новую функциональность;
- новые API;
- новые агенты;
- ML;
- LLM;
- DLP claims;
- SIEM claims;
- EDR claims.

---

Критерии приемки

- Demo Pack готов;
- Demo сценарии готовы;
- Demo screenshots актуальны;
- Demo report добавлен;
- Pilot Value Statement добавлен;
- документация согласована с реальным функционалом.

---

Финальный отчет

1. Добавленные документы.
2. Обновленные документы.
3. Demo сценарии.
4. Demo screenshots.
5. Demo dataset.
6. Проверки.
7. Ограничения.

## Выполнение

Статус: done.

Добавленные документы:

- `docs/demo/DEMO_SCENARIO_EXECUTIVE_RU.md`;
- `docs/demo/DEMO_SCENARIO_SECURITY_RU.md`;
- `docs/demo/DEMO_SCENARIO_FORENSICS_RU.md`;
- `docs/demo/DEMO_PACK_ACCEPTANCE_CHECKLIST_RU.md`;
- `docs/DEMO_REPORT_EXAMPLE_RU.md`;
- `docs/PILOT_VALUE_PROPOSITION_RU.md`.

Обновленные документы и материалы:

- `README.md`;
- `docs/DEMO_RUNBOOK_RU.md`;
- `docs/PILOT_DEMO_SCENARIO_RU.md`;
- `docs/PILOT_V1_EVIDENCE_RU.md`;
- `docs/fixtures/pilot-v1-demo/README_RU.md`;
- `docs/fixtures/pilot-v1-demo/demo-seed-data.json`.
- `scripts/detmir-pilot-demo-smoke.mjs`.

Demo dataset:

- синтетический;
- покрывает нормальную работу, снижение активности, рост удаленных сессий,
  повышенный UEBA, incident candidate и низкое покрытие агентами;
- использует только demo identifiers и TEST-NET адреса.

Demo screenshots:

- существующие PNG проверены как реальные изображения, не заглушки;
- обновление PNG не потребовалось.

Ограничения:

- новая функциональность, API, агенты, ML, LLM не добавлялись;
- SIEM/DLP/EDR claims не добавлялись;
- planned/future не описаны как implemented.
