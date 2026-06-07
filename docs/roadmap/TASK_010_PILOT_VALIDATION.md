.docs/roadmap/TASK_010_PILOT_VALIDATION.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: pilot validation
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: forbidden
New features: forbidden

Цель

Подготовить систему к реальному пилоту и выявить слабые места до внедрения.

Не добавлять новый функционал.

Проверить существующий.

---

Что реализовать

1. Pilot Validation Checklist

Создать:

docs/PILOT_VALIDATION_CHECKLIST_RU.md

Проверить:

- Executive сценарий;
- Workforce сценарий;
- Security сценарий;
- Forensics сценарий;
- Agent сценарий;
- Reporting сценарий.

---

2. Gap Analysis

Создать:

docs/PILOT_GAP_ANALYSIS_RU.md

Разделы:

Что готово

Что требует доработки

Что не входит в пилот

Что отложено на roadmap

---

3. Customer Questions Pack

Создать:

docs/CUSTOMER_DISCOVERY_QUESTIONS_RU.md

Вопросы для:

- директора;
- руководителя подразделения;
- ИБ;
- ИТ;
- эксплуатации.

---

4. Pilot Success Criteria

Создать:

docs/PILOT_SUCCESS_CRITERIA_RU.md

Определить:

- критерии успеха пилота;
- критерии провала пилота;
- KPI пилота;
- ожидаемый результат через 30 дней.

---

5. Competitive Positioning

Создать:

docs/COMPETITIVE_POSITIONING_RU.md

Сравнить:

- ActivityWatch
- Стахановец
- StaffCop
- SearchInform
- InfoWatch

Только честное сравнение.

Без маркетинговых заявлений.

---

6. Pilot Smoke

Создать:

scripts/pilot-validation-smoke.mjs

Проверить наличие:

- demo docs;
- registry docs;
- deployment docs;
- screenshots;
- roadmap;
- reports;
- runbooks.

---

Запрещено

Не делать:

- новые API;
- новый UI;
- новые агенты;
- ML;
- LLM;
- DLP claims;
- SIEM claims;
- EDR claims.

---

Критерии приемки

- документы созданы;
- smoke проходит;
- gap analysis составлен;
- success criteria определены;
- customer questions готовы.

---

Финальный отчет

1. Документы.
2. Выявленные пробелы.
3. Основные риски пилота.
4. Конкурентное позиционирование.
5. Проверки.

---

## Выполнение

Статус: выполнено.

Созданы документы:

- `docs/PILOT_VALIDATION_CHECKLIST_RU.md`
- `docs/PILOT_GAP_ANALYSIS_RU.md`
- `docs/CUSTOMER_DISCOVERY_QUESTIONS_RU.md`
- `docs/PILOT_SUCCESS_CRITERIA_RU.md`
- `docs/COMPETITIVE_POSITIONING_RU.md`

Добавлен smoke:

- `scripts/pilot-validation-smoke.mjs`

Обновлен README:

- добавлены ссылки на pilot validation пакет рядом с demo, registry и
  deployment материалами.

Выявленные пробелы:

- нужен live smoke на целевом стенде заказчика;
- нужно подтвердить покрытие источников данных и freshness;
- нужен контрольный backup and recovery тест;
- нужно проверить reverse proxy, TLS, авторизацию и внешнюю доступность портала;
- нужно закрепить владельцев действий по Recommended Actions.

Основные риски пилота:

- целевой стенд может отличаться от локального demo-стенда;
- неполное покрытие источников снизит confidence KPI;
- заказчик может ошибочно ожидать DLP/SIEM/EDR, если заранее не проговорить
  границы пилота;
- без владельцев действий управленческие рекомендации останутся отчетом без
  исполнения.

Конкурентное позиционирование:

- AWatch-rus позиционируется как Workforce Analytics + Security Analytics +
  Forensics;
- продукт не заявляется как замена ActivityWatch, Стахановец, StaffCop,
  SearchInform или InfoWatch;
- сравнение сделано честно: зрелые DLP/SIEM/employee monitoring продукты
  сильнее в глубине политик, сертификации и ширине enterprise-функций;
- сильная сторона AWatch-rus в пилоте: explainable KPI, Risk Narrative,
  role-based сценарии и прозрачные ограничения.

Проверки:

- `node --check scripts/pilot-validation-smoke.mjs`
- `node scripts/pilot-validation-smoke.mjs`
- `git diff --check`
