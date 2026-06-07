# docs/roadmap/TASK_013_DETMIR_PRODUCTION_VALIDATION.md

## Цель

Проверить реально работающий контур DetMir/AWatch-rus на нескольких пользователях и собрать фактические эксплуатационные данные.

Задача не про добавление функциональности.

Задача про проверку:

* что реально работает;
* что используется;
* где есть шум;
* где есть расхождение с документацией;
* какие риски есть перед расширением пилота.

---

## Контекст

Проект уже находится в состоянии:

* Demo Freeze v1;
* Pilot-ready;
* Registry-preparation-ready;
* Enterprise-deployment-documented.

При этом существует реально работающий контур DetMir на нескольких пользователях.

Нужно проверить именно его, не подменяя проверку demo/synthetic данными.

---

## Основные правила

Запрещено:

* добавлять новые API;
* добавлять новый UI;
* добавлять новые agent collectors;
* менять архитектуру;
* включать ML/LLM;
* добавлять DLP/SIEM/EDR claims;
* выгружать персональные данные в документы;
* коммитить реальные ФИО, логины, IP, hostname, названия подразделений заказчика;
* коммитить runtime artifacts, логи, дампы, скриншоты с реальными данными.

Разрешено:

* добавлять документацию;
* добавлять checklist;
* добавлять anonymized summary;
* добавлять smoke/validation scripts, если они не раскрывают данные;
* исправлять явные naming/documentation inconsistencies;
* фиксировать gaps как отдельные рекомендации.

---

## Что проверить

### 1. Runtime Health

Проверить работающий контур:

* `/healthz`;
* `/readyz`;
* `/version`;
* `/metrics`.

Зафиксировать:

* доступность;
* response status;
* наличие request id / correlation id;
* отсутствие 500;
* корректность metrics format.

Не сохранять реальные URL, IP, hostname.

---

### 2. Portal Usage Validation

Проверить вручную или через browser smoke:

* Executive Dashboard;
* Workforce view;
* Security view;
* Forensics view;
* Reports view.

Зафиксировать:

* какие страницы реально открываются;
* какие блоки отображаются;
* есть ли пустые/сломанные блоки;
* есть ли 500/404;
* есть ли визуальные проблемы.

Не коммитить screenshots с реальными данными.

Если нужны screenshots — сохранить только локально или сделать обезличенные.

---

### 3. KPI Validation

Проверить:

* Workforce KPI;
* Explainable KPI;
* Department Comparison;
* Trend Status.

Ответить:

* KPI выглядит правдоподобно или нет;
* explainability помогает понять KPI или нет;
* есть ли очевидно ложные/странные объяснения;
* есть ли недостаток данных;
* есть ли `confidence: low`.

---

### 4. UEBA / Risk Narrative / Action Center Validation

Проверить:

* UEBA Score;
* Risk Narrative;
* Recommended Actions.

Зафиксировать:

* есть ли шумные правила;
* есть ли бесполезные рекомендации;
* есть ли рекомендации без достаточной evidence;
* есть ли risk level, который выглядит завышенным;
* есть ли risk level, который выглядит заниженным.

Важно:

не исправлять правила в этой задаче, если это требует изменения логики.

Только зафиксировать findings.

---

### 5. Agent / Data Flow Validation

Проверить текущий runtime:

* работает ли текущий агентский контур;
* есть ли backlog/spool;
* есть ли ошибки flush;
* есть ли dead-letter;
* нет ли потери данных;
* heartbeat поступает или нет;
* данные доходят до портала/отчетов.

Если используются оба:

* `awatch-agent-rs`;
* `adk-rust/crates/awatch-agent`;

зафиксировать их фактические роли:

```text
legacy/current runtime:
new baseline core:
```

---

### 6. Performance Snapshot

Собрать обезличенную сводку:

* примерное число пользователей;
* примерное число событий/записей в сутки, если безопасно доступно;
* размер spool/backlog;
* время генерации report;
* время ответа основных API;
* наличие slow requests;
* наличие ошибок в logs.

Не коммитить raw logs.

---

### 7. Data Hygiene / Sensitive Data Audit

Проверить, что в репозитории и документах после работы не появились:

* реальные ФИО;
* реальные логины;
* реальные IP;
* реальные hostname;
* реальные подразделения заказчика;
* реальные screenshots;
* runtime logs;
* database dumps;
* персональные данные.

---

## Что создать

Создать документ:

```text
docs/DETMIR_PRODUCTION_VALIDATION_RU.md
```

Структура:

```text
# DetMir Production Validation

## Executive Summary

## Scope

## Environment

Обезличенно:
- пользователей: несколько;
- контур: working internal pilot;
- данные: реальные, но в документе не раскрываются.

## Runtime Health

## Portal Validation

## KPI Validation

## Explainable KPI Validation

## UEBA Validation

## Risk Narrative Validation

## Executive Action Center Validation

## Agent/Data Flow Validation

## Performance Snapshot

## Noise / False Positive Findings

## Documentation Mismatches

## Security / Privacy Notes

## Gaps

## Recommended Next Tasks

## Explicit Non-Goals

## Conclusion
```

---

## Что обновить

Обновить:

```text
docs/roadmap/TASK_013_DETMIR_PRODUCTION_VALIDATION.md
```

Добавить секцию:

```text
## Выполнение
```

с кратким итогом.

При необходимости обновить:

```text
docs/ROADMAP_CONFORMANCE_AUDIT_RU.md
```

только если найдены важные расхождения.

---

## Допустимые scripts

Если полезно, добавить:

```text
scripts/detmir-production-validation-smoke.mjs
```

Требования:

* не печатать реальные данные;
* не сохранять payload с персональными данными;
* проверять только статусы, наличие блоков и обезличенные счетчики;
* URL задавать через env:

```bash
DETMIR_VALIDATION_URL=http://127.0.0.1:8720
```

---

## Проверки

Выполнить:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
node scripts/deployment-readiness-smoke.mjs
node scripts/pilot-validation-smoke.mjs
AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs
```

Если добавлен новый script:

```bash
node --check scripts/detmir-production-validation-smoke.mjs
DETMIR_VALIDATION_URL=http://127.0.0.1:8720 node scripts/detmir-production-validation-smoke.mjs
```

Также выполнить:

```bash
git diff --check
```

и sensitive scan по добавленным/измененным файлам.

---

## Критерии приемки

Задача выполнена, если:

* создан `docs/DETMIR_PRODUCTION_VALIDATION_RU.md`;
* рабочий контур проверен без раскрытия персональных данных;
* health/ready/version/metrics проверены;
* portal проверен;
* KPI/explainability проверены;
* UEBA/Risk Narrative/Action Center проверены;
* agent/data flow проверен;
* performance snapshot зафиксирован обезличенно;
* gaps и recommended next tasks сформированы;
* sensitive data не попали в git;
* все проверки проходят.

---

## Финальный отчет Codex должен содержать

1. Что проверено.
2. Что подтверждено как работающее.
3. Какие gaps найдены.
4. Какие noisy rules/recommendations найдены.
5. Какие privacy/security ограничения соблюдены.
6. Какие документы созданы/обновлены.
7. Какие scripts добавлены.
8. Результаты проверок.
9. Рекомендованные следующие задачи.
