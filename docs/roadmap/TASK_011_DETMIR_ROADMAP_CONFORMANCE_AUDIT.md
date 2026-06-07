# docs/roadmap/TASK_011_DETMIR_ROADMAP_CONFORMANCE_AUDIT.md

## Рекомендуемые параметры

Mode: xhigh
Reasoning: maximum
Task type: conformance audit / product verification
Quality bar: production-ready
Breaking changes: forbidden
Architecture changes: forbidden
New features: forbidden
Code changes: discouraged unless fixing broken checks
Simplifications: forbidden

Required checks:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
node scripts/deployment-readiness-smoke.mjs
node scripts/pilot-validation-smoke.mjs
AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs
```

## Цель

Проверить, что фактически работающий DetMir/AWatch-rus соответствует текущему roadmap, документации, заявленным контрактам и демо-сценариям.

Не добавлять новую функциональность.

Главная задача — найти расхождения между:

* тем, что заявлено в roadmap;
* тем, что заявлено в README/docs;
* тем, что реально работает в API/портале/отчетах/агенте;
* тем, что проверяют smoke tests.

---

## Область проверки

Проверить соответствие следующих блоков:

1. Production hardening
2. Explainable KPI
3. Risk Narrative
4. Executive Action Center
5. Rust Agent Baseline
6. Demo Pack
7. Registry Readiness
8. Enterprise Deployment Guide
9. Pilot Validation readiness
10. Optional integrations positioning

---

## Что проверить

### 1. Roadmap Status Audit

Проверить файлы:

```text
docs/roadmap/
```

Для каждой задачи проверить:

* есть ли статус выполнения;
* соответствует ли статус реальному состоянию;
* есть ли ссылка на commit, если она уже добавлялась;
* нет ли задач со статусом done, но без реализации;
* нет ли реализованного функционала, который не отражен в roadmap.

Результат оформить в:

```text
docs/ROADMAP_CONFORMANCE_AUDIT_RU.md
```

---

### 2. README Claims Audit

Проверить README.md.

Найти:

* claims, которые подтверждаются кодом;
* claims, которые подтверждаются только документацией;
* claims, которые требуют уточнения;
* claims, которые нужно убрать или смягчить.

Особенно проверить, что README не заявляет:

* полноценную DLP;
* полноценную SIEM;
* EDR/XDR;
* ML/LLM scoring;
* обязательный pfSense;
* готовый React/Tauri UI;
* auto-remediation.

---

### 3. API Conformance Audit

Проверить, что реально существуют и работают endpoints:

```http
GET /healthz
GET /readyz
GET /version
GET /metrics
GET /api/reports
GET /api/workforce/kpi/explain
GET /api/risk/narrative
GET /api/actions
```

Проверить:

* JSON response shape;
* наличие request id / correlation id headers;
* role gates;
* query limits;
* report range limits;
* Prometheus metrics format.

---

### 4. Portal Conformance Audit

Проверить UI/portal routes.

Убедиться, что доступны и отображаются:

* Executive Dashboard;
* Workforce KPI;
* Explainable KPI block;
* Risk Narrative block;
* Recommended Actions block;
* Security view;
* Forensics view;
* reports view.

Если какой-то блок есть в API, но не виден в UI — зафиксировать как gap.

---

### 5. Markdown Report Audit

Проверить, что markdown reports содержат:

* Workforce KPI;
* Explainable KPI;
* Risk Narrative;
* Recommended Actions;
* limitations;
* no false DLP/SIEM/EDR claims.

---

### 6. Rust Agent Baseline Audit

Проверить новый crate:

```text
adk-rust/crates/awatch-agent/
```

Проверить наличие:

* config loader;
* telemetry envelope;
* heartbeat;
* local spool;
* retry/dead-letter;
* /healthz;
* /metrics;
* structured JSON logging;
* tests/smoke.

Проверить, что он не делает:

* keylogger;
* screenshot capture;
* clipboard capture;
* packet interception;
* kernel driver;
* EDR;
* DLP;
* ML/LLM.

Также проверить статус legacy/current runtime:

```text
awatch-agent-rs
```

Убедиться, что документация честно различает:

* current runtime;
* new baseline core.

---

### 7. Optional Integrations Audit

Проверить документы и README.

Убедиться, что pfSense описан только как:

```text
optional addon / contract_only / optional integration
```

Не как обязательная часть ядра.

Проверить также 1C, AD/LDAP, SIEM/syslog, external storage — только как optional/future/where configured.

---

### 8. Demo Pack Audit

Проверить:

```text
docs/demo/
docs/screenshots/
```

Проверить:

* все demo docs существуют;
* screenshots существуют;
* screenshots не заглушки;
* links валидны;
* нет реальных IP/ФИО/логинов/данных заказчика;
* используются только demo/test/example данные.

---

### 9. Registry Readiness Audit

Проверить:

```text
docs/REGISTRY_*.md
```

Проверить:

* core/optional/not claimed разделены корректно;
* нет юридических гарантий;
* нет заявления "готово к реестру" без оговорок;
* есть список remaining gaps;
* есть честное описание open-source dependencies/SBOM.

---

### 10. Enterprise Deployment Audit

Проверить:

* deployment guide;
* topologies;
* sizing;
* backup/recovery;
* operations runbook;
* security hardening;
* acceptance checklist.

Убедиться, что sizing не содержит неподтвержденных маркетинговых обещаний.

---

## Что создать

Создать итоговый отчет:

```text
docs/ROADMAP_CONFORMANCE_AUDIT_RU.md
```

Структура отчета:

```text
# Roadmap Conformance Audit

## Executive Summary

## Overall Status

## Confirmed Implemented Items

## Partially Implemented Items

## Documentation-Only Items

## Gaps

## False Claims Found

## API Verification

## Portal Verification

## Agent Verification

## Demo Pack Verification

## Registry Readiness Verification

## Deployment Readiness Verification

## Recommended Fixes

## Next Roadmap Corrections
```

---

## Допустимые изменения

Разрешено:

* исправлять документацию;
* исправлять markdown links;
* уточнять README claims;
* добавлять smoke checks;
* исправлять мелкие несоответствия, если они очевидны и не меняют архитектуру;
* обновлять roadmap status.

Нежелательно:

* добавлять новую функциональность;
* менять API;
* менять UI;
* менять agent behavior.

Если обнаружена серьезная проблема в коде — не исправлять молча, а зафиксировать как gap и предложить отдельную задачу.

---

## Запрещено

Не делать:

* новые API;
* новые UI-блоки;
* новые agent collectors;
* ML;
* LLM;
* DLP claims;
* SIEM claims;
* EDR claims;
* auto-remediation;
* обязательный pfSense;
* переписывание архитектуры.

---

## Критерии приемки

Задача выполнена, если:

* создан `docs/ROADMAP_CONFORMANCE_AUDIT_RU.md`;
* проверены roadmap/README/docs/API/portal/reports/agent/demo/registry/deployment;
* smoke checks проходят;
* ложные claims отсутствуют или исправлены;
* найденные gaps зафиксированы;
* предложены следующие корректировки roadmap.

---

## Финальный отчет Codex должен содержать

1. Краткое резюме аудита.
2. Подтвержденные реализованные блоки.
3. Частично реализованные блоки.
4. Documentation-only блоки.
5. Найденные gaps.
6. Исправленные claims/links, если были.
7. Результаты проверок.
8. Рекомендованные следующие задачи.
