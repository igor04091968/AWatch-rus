docs/roadmap/TASK_009_ENTERPRISE_DEPLOYMENT_GUIDE.md

Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: deployment readiness
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: forbidden
Simplifications: forbidden

Цель

Подготовить полный комплект документации и проверок для внедрения AWatch-rus в инфраструктуре заказчика.

Не добавлять новый функционал.

Использовать существующие возможности.

---

Что реализовать

1. Deployment Guide

Создать:

docs/ENTERPRISE_DEPLOYMENT_GUIDE_RU.md

Описать:

- архитектуру развертывания;
- минимальную инсталляцию;
- пилотную инсталляцию;
- рекомендуемую инсталляцию;
- ограничения.

---

2. Topology Examples

Создать:

docs/DEPLOYMENT_TOPOLOGIES_RU.md

Сценарии:

- standalone;
- pilot;
- small company;
- medium company;
- enterprise.

Для каждого сценария:

- компоненты;
- размещение;
- пример потоков данных.

---

3. Sizing Guide

Создать:

docs/SIZING_GUIDE_RU.md

Минимум:

- до 50 пользователей;
- до 250 пользователей;
- до 1000 пользователей;
- более 1000 пользователей.

Без маркетинговых обещаний.

Указать, что оценки требуют валидации.

---

4. Backup and Recovery

Создать:

docs/BACKUP_AND_RECOVERY_RU.md

Описать:

- резервирование конфигурации;
- резервирование отчетов;
- восстановление;
- ограничения.

---

5. Operations Runbook

Создать:

docs/OPERATIONS_RUNBOOK_RU.md

Описать:

- healthz;
- readyz;
- metrics;
- smoke;
- журналирование;
- типовые сбои;
- диагностику.

---

6. Security Hardening Guide

Создать:

docs/SECURITY_HARDENING_RU.md

Описать:

- TLS;
- reverse proxy;
- firewall;
- учетные записи;
- права доступа;
- журналирование.

Без ложных security claims.

---

7. Acceptance Checklist

Создать:

docs/ENTERPRISE_ACCEPTANCE_CHECKLIST_RU.md

Пункты:

- установка;
- запуск;
- доступность API;
- портал;
- smoke;
- документация;
- резервирование.

---

8. Deployment Smoke

Создать:

scripts/deployment-readiness-smoke.*

Проверять:

- наличие документации;
- наличие screenshots;
- наличие roadmap;
- наличие registry docs;
- наличие demo docs.

---

Запрещено

Не делать:

- новый код продукта;
- новые API;
- новые агенты;
- новые UI;
- новые claims;
- ML;
- LLM.

---

Критерии приемки

- документы созданы;
- ссылки валидны;
- smoke проходит;
- README обновлен при необходимости;
- deployment сценарии описаны;
- sizing описан;
- backup описан;
- runbook описан.

---

Финальный отчет

1. Список документов.
2. Список обновлений.
3. Deployment scenarios.
4. Sizing assumptions.
5. Backup model.
6. Runbook.
7. Acceptance checklist.
8. Проверки.
9. Ограничения.

## Выполнение

Статус: done.

Созданные документы:

- `docs/ENTERPRISE_DEPLOYMENT_GUIDE_RU.md`;
- `docs/DEPLOYMENT_TOPOLOGIES_RU.md`;
- `docs/SIZING_GUIDE_RU.md`;
- `docs/BACKUP_AND_RECOVERY_RU.md`;
- `docs/OPERATIONS_RUNBOOK_RU.md`;
- `docs/SECURITY_HARDENING_RU.md`;
- `docs/ENTERPRISE_ACCEPTANCE_CHECKLIST_RU.md`.

Созданный smoke:

- `scripts/deployment-readiness-smoke.mjs`.

Обновленные документы:

- `README.md`;
- `docs/roadmap/TASK_009_ENTERPRISE_DEPLOYMENT_GUIDE.md`.

Deployment scenarios:

- standalone;
- pilot;
- small company;
- medium company;
- enterprise.

Sizing assumptions:

- до 50 пользователей;
- до 250 пользователей;
- до 1000 пользователей;
- более 1000 пользователей;
- все оценки требуют проверки на инфраструктуре заказчика.

Backup model:

- config backup;
- reports/state backup;
- evidence metadata backup where used;
- restore test;
- rollback after upgrade.

Runbook:

- `/healthz`;
- `/readyz`;
- `/metrics`;
- smoke;
- logs;
- типовые сбои;
- диагностика.

Ограничения:

- новый продуктовый код, API, агенты и UI не добавлялись;
- ML/LLM не добавлялись;
- новые security claims не добавлялись;
- sizing не заявлен как гарантия без валидации.
