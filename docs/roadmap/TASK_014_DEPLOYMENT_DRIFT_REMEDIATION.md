# docs/roadmap/TASK_014_DEPLOYMENT_DRIFT_REMEDIATION.md

Цель:

Устранить расхождение между:

* Demo Freeze v1;
* live DetMir runtime.

Проверить и внедрить в рабочий контур:

* /healthz
* /readyz
* /version
* /metrics
* request/correlation id
* Explainable KPI
* Risk Narrative
* Executive Action Center

Проверить:

* почему endpoints дают 404;
* почему browser conformance падает;
* почему Executive layer отсутствует;
* соответствует ли развернутый runtime текущему main;
* не используется ли устаревший build.

Результат:

Не добавлять новые функции.

Добиться того, чтобы:

live runtime == documented runtime

и

live runtime == Demo Freeze v1

````

Критерий успеха очень простой:

Сегодня:

```text
Browser smoke
FAIL

Production hardening smoke
FAIL
````

После задачи:

```text
Browser smoke
PASS

Production hardening smoke
PASS
```

на живом контуре.

## Выполнение

Дата выполнения: 2026-06-07.

Статус: выполнено.

### Причина drift

Рабочий portal runtime на gateway host был запущен из устаревшего release
binary. Из-за этого live-контур не соответствовал Demo Freeze v1:

* production-hardening endpoints `/healthz`, `/readyz`, `/version`,
  `/metrics` возвращали `404`;
* отдельные API `/portal/api/workforce/kpi/explain`,
  `/portal/api/risk/narrative`, `/portal/api/actions` возвращали `404`;
* request/correlation headers отсутствовали на live API;
* browser conformance smoke видел старый Executive/Workforce/Security слой.

Кодовая база при этом уже содержала нужные контракты и UI-блоки. Проблема была
не в архитектуре и не в отсутствующем функционале, а в несовпадении deployed
binary с freeze-срезом.

### Ремедиация

Выполнен controlled deploy актуального release binary на gateway host:

* старый бинарник сохранен в backup-каталог на gateway host;
* новый release binary собран из текущей freeze-ветки;
* бинарник установлен в штатный путь portal service;
* `detmir-portal.service` перезапущен;
* rollback path сохранен через backup старого бинарника.

В репозиторий не добавлялись runtime payload, реальные logs, screenshots с
живыми данными, IP-адреса, hostname, логины, ФИО или подразделения.

### Live endpoint matrix после ремедиации

Проверено через gateway-local portal port:

| Endpoint | Результат |
| --- | --- |
| `/healthz` | `200` |
| `/readyz` | `200` |
| `/version` | `200` |
| `/metrics` | `200` |
| `/portal/api/health` | `200` |
| `/portal/api/reports?role=executive` | `200` |
| `/portal/api/workforce/kpi/explain` | `200` |
| `/portal/api/risk/narrative` | `200` |
| `/portal/api/actions` | `200` |

Также подтверждено:

* `X-Request-Id` возвращается;
* `X-Correlation-Id` возвращается;
* внешний gateway `/healthz` отвечает `200`;
* внешний `/portal/` остается закрыт авторизацией;
* свежий scan journal не показал panic, HTTP 500 или явных timeout.

### Smoke results

```text
AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:18720 \
node scripts/awatch-production-hardening-smoke.mjs
PASS
```

```text
AWATCH_BROWSER_SMOKE_URL=http://127.0.0.1:18720/portal/ \
AWATCH_BROWSER_SMOKE_ARTIFACT_DIR=/tmp/awatch-live-remediation-browser-smoke \
node scripts/browser-conformance-smoke.mjs
PASS
```

```text
DETMIR_PORTAL_SMOKE_URL=http://127.0.0.1:18720/portal/ \
node scripts/detmir-portal-tabs-smoke.mjs
PASS
```

### Test harness hardening

`scripts/browser-conformance-smoke.mjs` был усилен: после переключения роли он
ожидает фактические маркеры контента в `#content`, а не делает снимок через
фиксированную короткую задержку. Это устраняет ложный FAIL на холодной
асинхронной загрузке Executive/Security views.

Продуктовая бизнес-логика, API contracts, роли, scoring, collectors и
архитектура не менялись.

### Итог

```text
live runtime == documented runtime
live runtime == Demo Freeze v1
Browser smoke: PASS
Production hardening smoke: PASS
```
