.docs/roadmap/TASK_008_REGISTRY_READINESS.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: registry readiness / documentation / compliance packaging
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: forbidden
Simplifications: forbidden

Цель

Подготовить AWatch-rus к формальному позиционированию как российского программного продукта и будущей подаче в реестр российского ПО.

Не менять функциональность.

Собрать доказательную документационную базу.

---

Что реализовать

1. Product Passport

Создать:

docs/REGISTRY_PRODUCT_PASSPORT_RU.md

Включить:

- наименование продукта;
- назначение;
- функциональные модули;
- архитектура;
- стек технологий;
- режим поставки;
- состав ПО;
- системные требования;
- ограничения;
- что не является частью ядра.

---

2. Architecture Description

Создать:

docs/REGISTRY_ARCHITECTURE_RU.md

Описать:

- Rust backend;
- Rust agent baseline;
- HTML/HTMX portal;
- API contracts;
- reports;
- telemetry pipeline;
- optional integrations;
- pfSense как необязательный addon;
- future React/Tauri как roadmap, не текущий claim.

---

3. Functional Scope

Создать:

docs/REGISTRY_FUNCTIONAL_SCOPE_RU.md

Разделить:

Core:

- Workforce KPI
- Explainable KPI
- UEBA Score v1
- Risk Narrative
- Executive Action Center
- Reports
- Role-based portals
- Rust Agent baseline
- Demo Pack

Optional Addons:

- pfSense
- 1C
- AD/LDAP
- SIEM/syslog
- external storage

Not claimed:

- полноценная DLP
- полноценная SIEM
- EDR
- ML/LLM
- auto-remediation

---

4. Ownership / Dependency Statement

Создать:

docs/REGISTRY_DEPENDENCY_STATEMENT_RU.md

Описать:

- используемые open-source зависимости;
- SBOM assets;
- Cargo dependencies;
- отсутствие SaaS-критичной зависимости;
- что требуется проверить по лицензиям перед подачей.

---

5. Deployment Statement

Создать:

docs/REGISTRY_DEPLOYMENT_MODEL_RU.md

Описать:

- on-premise deployment;
- pilot deployment;
- local demo deployment;
- agent baseline deployment;
- systemd/Docker если уже есть;
- ограничения пилотной версии.

---

6. Commercial Positioning

Создать:

docs/REGISTRY_COMMERCIAL_POSITIONING_RU.md

Позиционирование:

AWatch-rus — Workforce-first платформа для:

- управленческого контроля активности;
- объяснимого KPI;
- риск-нарратива;
- поддержки ИБ;
- расследований;
- отчетности.

Не позиционировать как замену DLP/SIEM/EDR.

---

7. Registry Checklist

Создать:

docs/REGISTRY_READINESS_CHECKLIST_RU.md

Включить checklist:

- исходный код;
- сборка;
- документация;
- SBOM;
- лицензии;
- install guide;
- user guide;
- admin guide;
- demo pack;
- release assets;
- screenshots;
- функциональное описание;
- ограничения.

---

8. README Update

Обновить README кратким блоком:

Registry readiness documentation

со ссылками на новые документы.

---

Запрещено

Не делать:

- изменения кода;
- новые API;
- новые UI-функции;
- новые агенты;
- юридические гарантии;
- заявление “готово к реестру” без оговорок;
- claim полноценной DLP/SIEM/EDR;
- claim обязательного pfSense.

---

Критерии приемки

- документы созданы;
- README обновлен;
- ссылки валидны;
- нет ложных claims;
- pfSense описан как optional addon;
- future React/Tauri описаны только как roadmap;
- Registry checklist отражает текущие пробелы.

Проверки

Выполнить:

- markdown link check, если есть;
- git diff --check;
- sensitive scan по docs;
- проверить отсутствие реальных IP/ФИО/логинов.

Финальный отчет

1. Список созданных документов.
2. Список обновленных документов.
3. Что заявлено как core.
4. Что заявлено как optional.
5. Что явно не заявляется.
6. Какие пробелы остались до реальной подачи.

