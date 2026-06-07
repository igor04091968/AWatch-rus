# AWatch-rus Production Validation

Дата проверки: 2026-06-07.

Статус после TASK_014: рабочий внутренний pilot-контур приведен к Demo Freeze
v1 по portal runtime, production-hardening endpoints и основным live smoke.
Контур собирает и показывает реальные данные; расширение пилота допустимо
только контролируемо, после ручной проверки UEBA evidence и назначения
операционного ownership.

## Executive Summary

Проверка выполнялась read-only по рабочему внутреннему контуру на нескольких
пользователях. Реальные payload, логи, screenshots, IP-адреса, hostname,
логины, ФИО и подразделения в репозиторий не сохранялись.

Подтверждено:

- gateway и portal service доступны;
- базовый portal UI открывается;
- ActivityWatch API доступен и содержит свежие buckets;
- `/portal/api/reports` отвечает по ролям;
- Security events backend в рабочем контуре подключен;
- role gates в существующем portal smoke срабатывают;
- Forensics view и базовые portal tabs открываются;
- Windows runtime содержит активный текущий агентский процесс и watcher-процессы.

Ключевые gaps, найденные до TASK_014:

- фактический portal runtime отстает от Demo Freeze v1: отдельные endpoints
  `/portal/api/workforce/kpi/explain`, `/portal/api/risk/narrative` и
  `/portal/api/actions` на live-контуре возвращают `404`;
- production-hardening endpoints `/healthz`, `/readyz`, `/version`, `/metrics`
  не доступны на фактическом portal port; gateway-level `/healthz` отвечает,
  но это не заменяет portal production contract;
- request id / correlation id headers на live portal API не возвращаются;
- Executive visual conformance не проходит по текущему freeze smoke: в рабочем
  runtime не отображаются новые Pilot v1 блоки Risk Narrative / Explainable KPI
  / Recommended Actions;
- UEBA на live-контуре возвращает `critical` score; нужна ручная проверка
  evidence, чтобы отличить реальный риск от шумного правила.

TASK_014 remediation update:

- deployment/version drift закрыт controlled deploy актуального release binary;
- live `/healthz`, `/readyz`, `/version`, `/metrics` теперь отвечают `200`;
- request id / correlation id headers возвращаются;
- live Explainable KPI, Risk Narrative и Executive Action Center endpoints
  отвечают `200`;
- live production hardening smoke прошел;
- live browser conformance smoke прошел;
- live portal tabs smoke прошел.

Вывод: контур можно использовать для контролируемого внутреннего pilot review и
подготовки ограниченного пилота. Перед расширением на 10-50 пользователей
остается вручную разобрать UEBA `critical` evidence и закрепить порядок
операционной поддержки.

## Scope

Проверялось:

- runtime health;
- gateway/portal topology;
- portal tabs and role views;
- Workforce KPI и related report structure;
- Explainable KPI availability;
- UEBA;
- Risk Narrative availability;
- Executive Action Center availability;
- agent/data flow;
- performance snapshot;
- data hygiene.

Не проверялось:

- destructive recovery;
- restart/redeploy;
- изменение правил scoring;
- изменение collectors;
- production rollout новой версии;
- raw evidence review с персональными данными.

## Environment

Обезличенно:

- пользователей: несколько;
- контур: working internal pilot;
- данные: реальные, но в документе не раскрываются;
- gateway: отдельный reverse-proxy host с внешней авторизацией;
- portal runtime: локальный сервис на gateway host;
- ActivityWatch/worktime: отдельный AW-rus server;
- Windows runtime: RDP host с текущим агентским контуром.

## Runtime Health

Проверено:

| Поверхность | Результат | Комментарий |
| --- | --- | --- |
| Gateway `/healthz` | `200` | Nginx/gateway-level health отвечает |
| Gateway `/portal/` | `401` снаружи | Внешний доступ закрыт авторизацией |
| Portal local `/portal/` | `200` | UI доступен на gateway host |
| Portal local `/portal/api/health` | `200` | API health доступен |
| Portal local `/healthz` | `200` | Production-hardening endpoint подтвержден после TASK_014 |
| Portal local `/readyz` | `200` | Production-hardening endpoint подтвержден после TASK_014 |
| Portal local `/version` | `200` | Production-hardening endpoint подтвержден после TASK_014 |
| Portal local `/metrics` | `200` | Prometheus text metrics подтверждены после TASK_014 |
| ActivityWatch `/api/0/settings` | `200` | AW API отвечает |

Request/correlation headers после TASK_014:

- `X-Request-Id`: возвращается live portal API;
- `X-Correlation-Id`: возвращается live portal API.

Metrics:

- Prometheus metrics format на фактическом portal runtime подтвержден через
  `/metrics`.

## Portal Validation

Проверено через tunnel к фактическому gateway-local portal port. Screenshots
создавались только во временном каталоге вне репозитория и не коммитились.

Результат `scripts/browser-conformance-smoke.mjs` на live-контуре после
TASK_014:

| View | Результат | Комментарий |
| --- | --- | --- |
| Executive | OK | KPI, Explainable KPI, Risk Narrative и Recommended Actions отображаются |
| Workforce | OK | KPI, подразделения, тренды и explainability отображаются |
| Security | OK | Security actions, events, correlation и candidates отображаются |
| Forensics | OK | Расследования, timeline, материалы и аудит отображаются |

Результат `scripts/detmir-portal-tabs-smoke.mjs` на live-контуре после TASK_014:

- базовые tabs открываются;
- loading status доходит до ready;
- role switcher есть;
- Security events доступны;
- manager/security/forensics/admin view checks проходят;
- server role gates проходят;
- Executive dashboard layer и expected management block order проходят.

## KPI Validation

`/portal/api/reports?role=executive` возвращает валидный JSON и содержит:

- `kpis`: массив агрегированных KPI;
- `workforce`: объект с `department_comparison`, `owner_comparison`, `trend`,
  `trend_status`, `insights`;
- `business_risk`;
- `risk_heatmap`;
- `security_events_summary`.

Обезличенные счетчики live response:

- KPI entries: `13`;
- department comparison entries: `1`;
- owner comparison entries: `3`;
- business risk entries: `2`;
- risk heatmap entries: `7`.

Оценка:

- базовый Workforce/Business Risk слой на live-контуре присутствует;
- KPI выглядит как рабочий агрегированный отчет, но текущий UI/API не
  соответствует Demo Freeze v1 explainability контракту;
- перед расширением пилота нужно подтвердить свежесть источников по каждому
  пользователю и роль ожидаемых подразделений.

## Explainable KPI Validation

Live endpoint после TASK_014:

```text
/portal/api/workforce/kpi/explain -> 200
```

Вывод:

- Explainable KPI развернут на рабочем контуре;
- live UI показывает ожидаемый блок `Почему такой индекс активности?`;
- прежний `404` был следствием устаревшего deployed binary.

## UEBA Validation

Live endpoint:

```text
/portal/api/ueba -> 200
```

Обезличенная сводка:

- response `ok=true`;
- severity: `critical`;
- status: `FAIL`;
- score: `100`;
- reason codes: несколько;
- score components: несколько.

Оценка:

- UEBA endpoint работает;
- score `critical` требует ручной проверки evidence;
- без ручной проверки нельзя считать это подтвержденным нарушением;
- высокий score может быть как реальным риском, так и шумом от неполного
  покрытия/устаревших источников/политики baseline.

## Risk Narrative Validation

Live endpoint после TASK_014:

```text
/portal/api/risk/narrative -> 200
```

Вывод:

- Risk Narrative развернут на рабочем контуре;
- Executive UI block соответствует Demo Freeze v1 ожиданиям;
- Risk Narrative можно показывать как deployed capability, сохраняя честное
  ограничение: это rule-based объяснение, не ML-прогноз.

## Executive Action Center Validation

Live endpoint после TASK_014:

```text
/portal/api/actions -> 200
```

Вывод:

- Executive Action Center развернут на рабочем контуре;
- live runtime отдает отдельный actions endpoint;
- рекомендации можно показывать как deployed rule-based guidance, без claims об
  автоматическом remediation.

## Agent/Data Flow Validation

AW-rus server:

- ActivityWatch server active;
- worktime API service active;
- failed systemd units: `0`;
- ActivityWatch API buckets endpoint отвечает `200`;
- buckets count: `27`;
- latest bucket timestamp близок к моменту проверки;
- oldest bucket timestamp старый, что нормально для исторических/event buckets,
  но требует отдельной интерпретации freshness по bucket type.

Windows/RDP runtime:

- текущий `awatch-agent-rs` process активен;
- collector guard process активен;
- watcher/window/telemetry processes активны;
- scheduled tasks для ActivityWatch/AWatch runtime находятся в состоянии
  `Ready` или `Running`.

Фактические роли:

```text
legacy/current runtime: awatch-agent-rs и существующие ActivityWatch watchers
new baseline core: adk-rust/crates/awatch-agent, покрыт тестами, но не подтвержден как основной live runtime
```

Backlog/dead-letter:

- явный dead-letter count на AW server: `0`;
- known spool directories на AW server не обнаружены в проверенных путях;
- Windows-side spool/backlog требует отдельной безопасной проверки без вывода
  путей и payload.

## Performance Snapshot

Обезличенная сводка:

| API | Status | Время ответа |
| --- | --- | --- |
| `/portal/api/health` | `200` | < 1 ms на gateway-local check |
| `/portal/api/reports?role=executive` | `200` | первый observed run около 9 s, warm-cache run < 10 ms |
| `/portal/api/reports?role=manager` | `200` | < 10 ms на warm-cache run |
| `/portal/api/reports?role=security` | `200` | < 10 ms на warm-cache run |
| `/portal/api/reports?role=forensics` | `200` | < 10 ms на warm-cache run |

Логи:

- recent portal log scan за окно проверки не показал `500`, panic или явных
  timeout в sanitized summary;
- ActivityWatch/worktime recent error scan не показал явных ошибок в sanitized
  summary.

Ограничение:

- это snapshot, не load test и не sizing report.

## Noise / False Positive Findings

Потенциальный шум:

- UEBA severity `critical` / score `100` без ручной валидации evidence может
  выглядеть завышенным;
- stale исторические buckets могут искажать общее восприятие freshness, если не
  разделять active, inactive и event-driven bucket types;
- live Executive headline/runtime naming все еще может содержать старую
  внутреннюю терминологию, что конфликтует с public naming hygiene.

Не исправлялось в этой задаче:

- scoring rules;
- UEBA thresholds;
- report content logic;
- deployed binary/runtime.

## Documentation Mismatches

Расхождения, найденные TASK_013, закрыты TASK_014:

1. Production-hardening endpoints `/healthz`, `/readyz`, `/version`,
   `/metrics` теперь доступны на фактическом portal port.
2. Standalone endpoints `/api/workforce/kpi/explain`,
   `/api/risk/narrative`, `/api/actions` теперь доступны на live runtime.
3. Visual smoke текущей freeze-ветки проходит Executive, Workforce, Security и
   Forensics views.
4. Live runtime обновлен controlled deploy актуального release binary.

## Security / Privacy Notes

Соблюдено:

- raw logs не коммитились;
- raw JSON payload не коммитился;
- screenshots с реальными данными не коммитились;
- реальные IP/hostname/usernames/ФИО/подразделения в этот документ не внесены;
- deploy выполнялся controlled способом с backup старого binary и rollback path;
- destructive data operations не выполнялись;
- collectors, scoring rules и источники данных не менялись.

## Gaps

Критично перед расширением пилота:

1. Провести ручной разбор UEBA `critical` с evidence, не меняя правила вслепую.
2. Назначить операционного владельца live smoke, deploy parity и rollback.
3. Зафиксировать регламент: после каждого deploy проверять binary/version,
   endpoint matrix и live smoke.

Желательно до пилотного расширения:

1. Добавить отдельный pilot-feedback контур для замечаний руководителя, ИБ,
   эксплуатации и расследователей.
2. Разделить freshness report по bucket types: active, inactive, event-driven,
   historical.
3. Проверить Windows-side spool/backlog безопасной командой без раскрытия путей
   и payload.

Можно перенести после первого ограниченного пилота:

1. Тонкая настройка UEBA thresholds.
2. Расширение Action Center rules.
3. Улучшение dashboard wording по результатам реальной обратной связи.

## Recommended Next Tasks

Не открывать feature roadmap. Следующие задачи должны быть pilot-feedback /
operations oriented:

- `docs/pilot-feedback/BUGS.md` - зафиксировать deployment/version drift как bug;
- `docs/pilot-feedback/FEATURE_REQUESTS.md` - собирать только запросы от
  реальных ролей;
- `docs/pilot-feedback/LESSONS_LEARNED.md` - фиксировать, что было непонятно на
  показе;
- отдельная operator task: сверить deployed portal binary/commit с freeze branch;
- отдельная operator task: повторить live smoke после controlled deploy.

## Explicit Non-Goals

В рамках TASK_013/TASK_014 не выполнялись:

- новые API;
- новый UI;
- новые collectors;
- ML/LLM;
- DLP/SIEM/EDR claims;
- изменение scoring logic;
- изменение collectors или источников данных;
- выгрузка персональных данных;
- сохранение real screenshots в git.

## Conclusion

Рабочий внутренний контур AWatch-rus существует и собирает реальные данные.
Portal, ActivityWatch, Security events, UEBA endpoint, Forensics и базовые role
views частично подтверждены.

После TASK_014 live runtime соответствует Demo Freeze v1 по production-hardening
endpoints, request/correlation headers, Explainable KPI, Risk Narrative,
Executive Action Center и основным smoke-проверкам. Текущий статус:

```text
ready for controlled internal pilot review;
ready for limited pilot preparation after manual UEBA evidence review and
operations ownership assignment.
```
