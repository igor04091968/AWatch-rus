# Аудит готовности AWatch-rus к пилотной эксплуатации

Дата аудита: `2026-06-05`

Объект аудита: AWatch-rus, Rust workspace `adk-rust`, портал
`detmir-portal`, агент `awatch-agent-rs`, документы коммерческого пилота и
реестровой подготовки.

Ограничения аудита: архитектура не менялась, новые сущности не добавлялись,
рефакторинг не выполнялся. Проверка выполнена как readiness-аудит к
контролируемому пилоту, а не как сертификационная экспертиза СЗИ.

## 1. Итоговая оценка

Статус: `ГОТОВ К КОНТРОЛИРУЕМОМУ ДЕМОНСТРАЦИОННОМУ ПИЛОТУ`

Оценка готовности: `90 / 100`

Вывод:

- Для ограниченного пилота на заранее согласованном контуре критических
  технических блокеров в коде и портале не выявлено.
- Полный демонстрационный путь "главный риск -> подразделение -> кандидат ->
  расследование -> пакет -> отчет" проверен на рабочем AWatch-rus-контуре; перед
  показом нужен только короткий преддемо-прогон на той же сети и экране.
- Для широкого промышленного внедрения остаются обязательные доработки:
  формальный контур доступа/RBAC, backup/retention для файловых state,
  sizing/load-тесты, API/schema versioning и регламент эксплуатации агента.
- Текущая продуктовая линия корректная: Workforce-first, операционный контроль,
  технический аудит, explainable risk и расследования без заявления продукта как
  сертифицированной DLP/SIEM/EDR/СЗИ.

## 2. Выполненные проверки

| Проверка | Результат |
|---|---|
| `cargo test --workspace` | OK после локального переноса Cargo target на Linux-ФС: `<LOCAL_CARGO_TARGET_DIR>`. Старый `target/` на `fuseblk` не подходит для `libsqlite3-sys`. |
| `cargo clippy --all-targets --all-features` | OK |
| `cargo build --release` | OK |
| `node scripts/detmir-portal-tabs-smoke.mjs` против `<PORTAL_URL>` | OK; security events доступны через ClickHouse, переход к расследованию проверен, найдено 3 кнопки расследования. |
| `GET /portal/api/reports` на пустом state-dir | OK, валидный JSON |
| Отсутствие `expected_nodes.json` | OK, `agent_coverage_sla.sla_status=UNKNOWN` |
| Отсутствие `incident_reviews.json`, `incident_review_audit.jsonl`, `cases.json` | OK, портал не падает |
| `local_fallback` в тестах агента/портала | OK, не подтверждает KPI |
| `collector_error` в тестах портала | OK, виден в explain/markdown |

Portal smoke подтвердил:

- вкладки `Обзор`, `Сотрудники`, `Подразделения`, `Риски`, `Расследования`,
  `Сетевой периметр`, `Отчеты`, `Настройки` открываются;
- Risk Narrative выводится первым;
- read-only настройки отображают период, рабочий день, русские названия
  порогов и источник правил;
- видимые статусы портала переведены с `OK/WARN/FAIL/UNKNOWN` на русские
  формулировки; технические подсказки ClickHouse/env убраны из пользовательских
  экранов;
- переход "кандидат -> расследование" проверен: smoke нашел 3 кнопки перехода
  и открыл карточку расследования;
- события безопасности читаются порталом через ClickHouse:
  `backend=clickhouse`, `status=ok`, `fallback_used=false`;
- первый расчет отчета прогревается при старте `detmir-portal.service`; после
  прогрева `/api/reports` отвечает за доли секунды;
- фоновое обновление больше не переводит готовый экран в состояние
  "Загрузка данных"; 70-секундная браузерная проверка сохранила `READY`;
- мобильная проверка 390px прошла: `READY`, глобального горизонтального overflow
  нет.

## 3. Архитектура

Сильные стороны:

- Архитектура уже выстроена как цепочка `Agent -> Telemetry -> Analytics ->
  Risk -> Investigation -> Report`.
- Rust-first runtime покрывает критичные серверные helpers, DLP/worktime paths,
  readiness, portal и собственный агент.
- pfSense/network perimeter оставлен optional/read-only слоем, что снижает риск
  пилота и соответствует текущим ограничениям проекта.
- Python сохранен только для оговоренных исключений, не как ядро продукта.
- Документы фиксируют корректное позиционирование: не СЗИ/DLP/SIEM, а платформа
  операционного контроля, технического аудита и Workforce/UEBA-аналитики.

Слабые стороны:

- Архитектура пока сильно завязана на набор сервисов/утилит и файловых
  интеграций; единая production control-plane модель еще не оформлена.
- Часть интеграций исторически выросла из ops-скриптов; runtime уже Rust-first,
  но границы владения между portal, AW, DLP, readiness и worktime требуют
  отдельной эксплуатационной схемы.
- Для пилота это приемлемо, но для масштабирования потребуется явная схема
  сервисов, портов, state-файлов, владельцев данных и recovery flows.

Оценка: `хорошо для пилота`, `нужно усилить для масштабного rollout`.

## 4. API

Сильные стороны:

- `GET /api/reports` устойчиво собирает executive dashboard, Risk Narrative,
  Trust KPI, agent quality, coverage SLA, business risk, heatmap, correlation,
  candidates, cases и markdown.
- `POST /api/telemetry` защищен API key/Bearer token и не принимает дефолтный
  `change-me`.
- Старые telemetry payload без diagnostics не ломают отчеты: качество данных
  становится `UNKNOWN`.
- Incident Review и Cases не создают инциденты автоматически; workflow остается
  ручным и проверяемым.

Слабые стороны:

- Контракты API уже доступны через `/api/contracts`, OpenAPI и TypeScript,
  но нужна формальная матрица версий и журнал совместимых/несовместимых
  изменений полей.
- Нет отдельного lightweight endpoint для части управленческих данных; `/api/reports`
  остается большим агрегирующим endpoint.
- Авторизация пользователя портала предполагается внешним gateway; сам портал не
  реализует полноценный RBAC.

Риск пилота: средний. Для контролируемого стенда допустимо, для заказчика нужен
фиксированный API contract и контур доступа.

## 5. Portal UI

Сильные стороны:

- Портал уже выглядит как управленческий контур: сначала связанная картина риска,
  затем сводка руководителя, доверие к данным, риски, кандидаты и дела.
- UI smoke подтверждает работу всех основных вкладок.
- Risk Narrative связывает Trust KPI, Agent Coverage, Business Risk, Heatmap,
  Security Correlation, Incident Candidates и Cases.
- Для руководителя важные выводы формулируются человеко-понятно, а не только
  техническими метриками.

Слабые стороны:

- Разделение ролей пока в основном интерфейсное и организационное; enforcement
  должен выполняться внешним auth/RBAC-контуром.
- На пустых данных портал работает, но часть выводов ожидаемо имеет статус
  `UNKNOWN`/`ATTENTION`; перед демо нужен подготовленный demo/pilot dataset.
- PDF/export-путь требует отдельной приемочной проверки в конкретном окружении.
- Документы для заказчика все еще требуют языковой чистки от англоязычных
  терминов и технических сокращений.

Оценка: `готов к демонстрации и пилоту при наличии auth gateway`.

## 6. Rust Agent

Сильные стороны:

- `awatch-agent-rs` имеет единую модель `TelemetryRecord` для Windows/Linux/FreeBSD.
- Есть retry/backoff/spool: кратковременный обрыв связи не обязан приводить к
  потере данных.
- Session quality уже поднят до уровня portal/report: `wts_api`,
  `quser_utf16`, `quser_lossy`, `env_sessionname_fallback`, `local_fallback`.
- `local_fallback` явно считается диагностическим режимом и не подтверждает KPI.
- Тесты покрывают дедупликацию сессий, local_fallback, spool, конфиг и
  сериализацию.

Слабые стороны:

- Windows collector промышленного уровня еще требует расширения вокруг Event Log,
  ETW/WMI и устойчивой установки как service с централизованным rollout/rollback.
- FreeBSD/pfSense mode находится на read-only foundation уровне и не должен
  продаваться как законченный firewall/NAC enforcement.
- Нужен регламент мониторинга spool backlog, версии агента и качества данных по
  рабочим местам.

Оценка: `достаточно для пилотного сбора worktime/RDP`, `не завершено как полный
enterprise endpoint agent`.

## 7. JSON-хранилища и state

Проверенные state-файлы:

- `telemetry.jsonl`
- `data/incident_reviews.json`
- `data/incident_review_audit.jsonl`
- `data/cases.json`
- `config/expected_nodes.json`

Сильные стороны:

- Отсутствующие файлы не ломают портал.
- Audit trail для incident review append-only по смыслу и не удаляет историю.
- Cases создаются только вручную из подтвержденных candidates.
- Пустой state-dir возвращает валидный `/api/reports`.

Слабые стороны:

- JSON/JSONL storage пока годится для пилота, но не для высокой конкуренции
  записи, больших объемов и долгого retention без ротации.
- Нужны регламенты backup/restore, lock/atomic-write policy, ротация audit JSONL
  и контроль размера telemetry history.
- Нужна миграционная политика state schema.

Риск пилота: средний. Для 1-2 подразделений допустимо, для широкого внедрения
нужен storage hardening или перенос критичного state в SQLite/DB.

## 8. Документация

Сильные стороны:

- Есть документы по архитектуре, порталу, security model, agent architecture,
  deployment, Business Risk, pilot checklist, release readiness и позиционированию.
- Документация удерживает безопасную линию для реестра: прикладные ИБ/DLP-lite
  функции не заявляются как сертифицированная СЗИ.
- Pilot checklist уже содержит доступ, scope, retention, Grafana, evidence,
  readiness и приемку.

Слабые стороны:

- Нет единого “операторского пакета пилота” в одном маршруте: installation ->
  first telemetry -> validation -> demo -> acceptance.
- API-контракты уже оформлены отдельными маршрутами, но документация должна
  явно ссылаться на OpenAPI/TypeScript и порядок проверки совместимости.
- Не хватает sizing guide: число endpoints, объем telemetry/day, CPU/RAM/disk,
  рекомендуемый retention.

Оценка: `хорошая база`, `нужно уплотнить до customer pilot pack`.

## 9. Отказоустойчивость

Сильные стороны:

- Агент имеет spool/retry.
- Портал best-effort читает optional state и не падает при отсутствии
  telemetry/review/audit/cases/expected_nodes.
- Readiness bundle, checksum/signature и Prometheus/Grafana readiness ideas уже
  заложены в документах и тестах.
- Legacy fallback сохранен там, где это нужно для rollback.

Слабые стороны:

- Нет HA для портала/AW server/хранилищ.
- Нет формального RTO/RPO.
- Нет нагрузочного теста на массовый прием telemetry и генерацию `/api/reports`.
- Нет отдельной очереди с backpressure на серверной стороне telemetry ingest.

Оценка: `достаточно для контролируемого пилота`, `не достаточно для SLA-продажи`.

## 10. Обратная совместимость

Сильные стороны:

- Старые telemetry без diagnostics работают.
- `agent_quality` сохранен, новые explain/history/nodes поля добавлены без
  поломки старого API.
- Отсутствие JSON state файлов обрабатывается.
- PowerShell collector сохранен как legacy fallback, а не удален.

Слабые стороны:

- Нет формальной матрицы совместимости версий agent/server/portal.
- Нет version negotiation для telemetry payload.
- Нужен changelog breaking/non-breaking API полей.

Оценка: `практически совместимо`, `формально не закреплено`.

## 11. Производительность

Сильные стороны:

- Rust-gates проходят быстро, release build чистый.
- Критичные runtime helpers переведены на Rust.
- Portal snapshot cache снижает повторное выполнение внешних команд.

Слабые стороны:

- `/api/reports` является агрегирующим endpoint и потенциально станет тяжелым
  при росте telemetry history, cases, candidates и audit JSONL.
- JSONL scan без ограниченной индексации может стать узким местом.
- Нет зафиксированного benchmark: endpoints, records/day, время формирования
  отчета, p95/p99 latency.

Оценка: `достаточно для пилота`, `нужны load-тесты перед масштабированием`.

## 12. Безопасность

Сильные стороны:

- Публичная документация придерживается sanitized-подхода и не должна содержать
  live infrastructure values.
- Evidence path защищен opaque ID, canonical path/root allowlist, magic
  validation, max-size limit, hash validation и Bearer upload token.
- Telemetry ingest не принимает дефолтный ключ.
- `local_fallback` не используется как доказательство активности.
- pfSense mutation/enforcement не включен в текущий scope.

Слабые стороны:

- Portal auth/RBAC должен быть гарантирован внешним gateway; без него портал не
  должен публиковаться наружу.
- Нужны security headers, TLS termination profile, session/access log policy.
- Нужно определить, кто может менять Incident Review/Cases и как проверяется
  `reviewer`.
- JSON state и evidence нуждаются в отдельном backup/retention/access-control
  регламенте.

Оценка: `нормально для закрытого пилота за auth gateway`, `нельзя открывать как
самостоятельный публичный портал без gateway/RBAC`.

## 13. Сильные стороны проекта

- Сформирована понятная коммерческая ценность: Workforce-first + Security +
  Forensics.
- Портал показывает не только метрики, а причинно-следственную картину риска.
- Rust-first перенос выполнен широко: agent, portal, DLP/worktime/readiness,
  helpers и проверки.
- Есть доказательная линия: agent quality, coverage SLA, candidates, incident
  review audit, investigation packs, cases.
- Тесты закрывают ключевые регрессии: old telemetry, missing state, fallback,
  collector errors, coverage SLA, markdown order.
- Публичное позиционирование стало безопаснее для реестра и коммерческой
  презентации.

## 14. Слабые стороны

- Нет встроенного RBAC/auth в portal application layer.
- JSON/JSONL state пока не hardened для больших объемов и параллельных записей.
- API-контракты и OpenAPI уже есть, но нет формального versioning и матрицы
  совместимости agent/server/portal.
- Нет load/sizing профиля.
- Windows agent еще требует промышленного rollout guide и матрицы OS/locale.
- FreeBSD/pfSense support нельзя считать законченным commercial module.
- Не хватает единого customer pilot runbook с шагами “чистая VM -> агенты ->
  первый отчет -> приемка”.

## 15. Технический долг

1. Описать и зафиксировать API schema/versioning.
2. Ввести storage policy для JSON/JSONL: lock, atomic write, rotation, backup,
   restore, retention.
3. Разделить heavy `/api/reports` на стабильные contract endpoints или
   документировать его как aggregate endpoint с p95 SLA.
4. Добавить benchmark/load-test сценарий telemetry ingest и report generation.
5. Оформить RBAC/auth gateway profile для ролей: владелец, руководитель, ИБ,
   оператор, администратор.
6. Сделать matrix agent compatibility: OS, locale, session source, fallback,
   worktime accepted/not accepted.
7. Подготовить anonymized pilot dataset и регламент преддемо-прогона.

## 16. Риски пилота

| Риск | Уровень | Что сделать до пилота |
|---|---|---|
| Неверная трактовка proxy KPI как абсолютной “полезности” | HIGH | В договоре/демо явно писать: индекс активности - proxy, веса приложений согласуются по ролям. |
| Недостаточное покрытие агентов | HIGH | Заполнить `expected_nodes.json`, проверять coverage SLA каждый день. |
| Портал без auth gateway | CRITICAL | Не публиковать портал без TLS/auth/RBAC reverse proxy. |
| Рост JSONL/audit файлов | MEDIUM | Включить retention/backup/rotation policy. |
| Спорные candidates из-за плохого data quality | MEDIUM | Использовать agent quality, `local_fallback` не принимать в KPI, показывать Trust KPI. |
| Неописанная версия agent/server | MEDIUM | Зафиксировать версии в pilot acceptance и release manifest. |
| Evidence/privacy вопросы | HIGH | Согласовать локальный регламент, доступы, retention, уведомление сотрудников. |

## 17. Блокеры внедрения

Для контролируемого пилота:

- Критических блокеров не выявлено, если портал закрыт auth gateway и scope
  пилота ограничен заранее выбранными рабочими местами.

Для промышленного customer rollout:

1. Нет формального production auth/RBAC profile для портала.
2. Нет утвержденного backup/retention/access-control регламента для telemetry,
   cases, audit и evidence.
3. Нет sizing/load-test отчета.
4. Нет формального API versioning и матрицы совместимости agent/server/portal.
5. Нет завершенного customer pilot runbook с ожидаемыми результатами проверки.

## 18. Рекомендации v0.3

Приоритет P0 до пилота:

1. Подготовить закрытый auth gateway profile: TLS, Basic/OIDC/VPN, role mapping,
   access logs.
2. Заполнить `expected_nodes.json` для пилотного scope и проверить coverage SLA.
3. Описать pilot retention: telemetry, reports, incident review audit, cases,
   evidence.
4. Зафиксировать версии agent/server/portal и команды проверки в pilot
   acceptance act.
5. Перед показом выполнить преддемо-прогон: готовность данных, наличие
   кандидата, открытие расследования, скачивание пакета и итоговый отчет.

Приоритет P1 для v0.3:

1. Зафиксировать API versioning, матрицу совместимости и порядок проверки
   OpenAPI/TypeScript contracts.
2. Добавить storage hardening для JSON/JSONL state или перенести критичный state
   в SQLite.
3. Сделать load test: 50/100/250 endpoints, records/day, `/api/reports` p95.
4. Расширить agent deployment docs: Windows service, rollback, locale matrix,
   spool monitoring.
5. Добавить единый `PILOT_RUNBOOK_RU.md`: установка, проверка, smoke, отчет,
   критерии приемки.

Приоритет P2 после пилота:

1. Развивать per-user baseline и role-based workforce weights.
2. Укрепить FreeBSD/pfSense read-only mode, но не включать enforcement без
   отдельного решения.
3. Добавить SIEM/syslog/webhook export profile как integration pack.
4. Добавить formal evidence chain policy, если продукт будет продаваться как
   Forensics-ready.

## 19. Финальный вывод

AWatch-rus уже можно показывать заказчику как MVP+/v0.3 платформу
Workforce-first операционного контроля с explainable risk и ручным
расследовательским workflow.

Для пилота система должна запускаться в закрытом контуре, с заранее
согласованными рабочими местами, включенным agent coverage SLA, заполненным
списком expected nodes и внешним auth gateway перед порталом.

Главная техническая граница зрелости сейчас не в UI и не в Rust-переносе, а в
эксплуатационной дисциплине: доступы, retention, backup, API contract, sizing и
регламент качества данных агента.
