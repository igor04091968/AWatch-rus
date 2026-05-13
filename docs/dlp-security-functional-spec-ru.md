# AWatch-rus DLP: реализованный функционал для службы ИБ

Документ описывает фактически реализованный DLP и смежный контрольный функционал в репозитории `AWatch-rus` по состоянию на текущий `main`.

- Статус: действующая реализация, не roadmap
- Назначение: аудит службой ИБ, эксплуатационное понимание, оценка рисков
- Контур: Windows/RDP endpoint collectors + Linux AW server + WebUI overlay + интеграции + отчётность

## 1. Границы системы

Система построена поверх `ActivityWatch` и расширяет его до прикладного DLP/monitoring-контура:

- сбор активности пользователей на Windows/RDP-хостах;
- DLP-сигналы по каналам `clipboard`, `USB`, `print`, `browser domains`, `email outbound`, `file operations`;
- централизованная политика DLP;
- review/rules UI внутри AW WebUI;
- кейсы расследований;
- интеграции в SIEM/SOAR;
- compliance-отчёты;
- health-check и autoheal для production-эксплуатации.

Система не является полноценной DLP-платформой enterprise-класса с нативной аутентификацией, RBAC, аппаратной изоляцией и криптографической подписью политик. Это важно учитывать при ИБ-оценке.

## 2. Реализованные компоненты

### 2.1 Windows endpoint / RDP host

Основные PowerShell-компоненты:

- `windows/dlp-endpoint-signals-collector.ps1`
  Сбор и DLP-оценка `clipboard`, `USB`, `print`.
- `windows/file-operations-collector.ps1`
  Сбор файловых операций и heartbeat состояния коллектора.
- `windows/browser-domains-native-collector.ps1`
  Сбор активных доменов/категорий браузера, генерация DLP-инцидентов по web-правилам.
- `windows/email-outbound-collector.ps1`
  Мониторинг исходящей почты, публикация почтовых событий и DLP-инцидентов.
- `windows/worktime-session-collector.ps1`
  Сбор состояния RDP-сеансов через `query user`/`quser`.
- `windows/dlp-policy-client.ps1`
  Pull-клиент централизованной политики.
- `windows/install-dlp-client.ps1`
  Простой инсталлятор клиента.
- `windows/install-standalone-service.ps1`
  Развёртывание DLP-агента как Windows Service.
- `windows/aw-standalone-service.ps1`
  Service wrapper для поддержания collector-процессов без Task Scheduler.

Deployment/tooling:

- `windows/deploy-single-user.ps1`
- `windows/deploy-domain-users.ps1`
- `windows/deploy-ensemble.ps1`
- `windows/hardening-recovery.ps1`
- `windows/validate-deployment.ps1`

### 2.2 Linux AW server

Базовые серверные компоненты:

- `aw-server/aw-ru-patch.js`
  RU/DLP overlay для WebUI.
- `aw-server/apply_webui_ru_patch.sh`
  Применение WebUI-патча.
- `aw-server/aw-worktime-api.py`
  API отчётов worktime на `:5610`.
- `aw-server/aw-worktime-ui-bridge.py`
  Мост между `aw-worktime-sessions_*` и стандартными AW-представлениями.
- `aw-server/aw-worktime-autoheal.sh`
  Автолечение worktime-представлений.

### 2.3 Policy Engine

Каталог: `aw-server/dlp-policy-engine/`

- `policy_service.py`
  FastAPI service централизованной политики.
- `policy_storage.py`
  SQLite storage и versioning.
- `policy_schema.py`
  Pydantic schemas.
- `policy_distributor.py`
  Формирование policy bundle для endpoint.
- `dlp-policy-engine.service`
  systemd unit.

### 2.4 Content Analysis

Каталог: `aw-server/dlp-content-analysis/`

- `content_analyzer.py`
  Унифицированный server-side анализ текста/артефактов.
- `dictionary_matcher.py`
  Match по словарям и regex pack.
- `checksum_validator.py`
  Валидация ИНН/СНИЛС/паспортных паттернов.
- `ocr_processor.py`
  OCR через `pytesseract` + `Pillow`.
- `dictionaries/152-fz-pdn.json`
  Словарь ПДн.
- `regex-packs/*.json`
  Наборы regex для `financial`, `contacts`, `secrets`.

### 2.5 Case Management

Каталог: `aw-server/dlp-case-management/`

- `case_service.py`
  FastAPI API для кейсов.
- `case_storage.py`
  SQLite-хранилище кейсов, комментариев, аудита.
- `case_schema.py`
  Схемы API.
- `evidence_chain.py`
  Нормализация evidence и вычисление `sha256`.
- `case-service.service`
  systemd unit.

### 2.6 SIEM / SOAR

Каталог: `aw-server/dlp-integrations/`

- `cef_exporter.py`
  Экспорт `aw-dlp-incidents_*` в CEF.
- `webhook_sender.py`
  Отправка webhook по severity.
- `syslog_forwarder.py`
  Generic syslog-forwarding инцидентов.
- `cef-config.yaml`
- `webhook-config.yaml`
- `syslog-forwarder-config.yaml`
- service/timer units для каждого интеграционного потока.

### 2.7 Compliance / reporting

Каталог: `aw-server/dlp-compliance/`

- `report_generator.py`
  Генератор месячных compliance-отчётов.
- `compliance_scheduler.py`
  Scheduler wrapper.
- `templates/152-fz-report.html`
- `templates/pci-dss-report.html`
- `report-scheduler.service`
- `report-scheduler.timer`

### 2.8 IOC enrichment

Сценарии и артефакты:

- `scripts/extract_ioc_from_sigma.py`
  Извлечение IOC из Sigma/Hayabusa rules.
- `scripts/build_dlp_ioc_from_hayabusa.sh`
  Построение JSON/CSV/SQL артефактов IOC.

### 2.9 Health / autoheal / operations

- `aw-server/health-check.sh`
  Базовый AW health gate.
- `scripts/dlp-health-check.py`
  DLP health gate.
- `scripts/diag_and_manual_restart.sh`
  Диагностика и ручной heal/restart.
- `scripts/dlp-admin-cli.py`
  CLI администратора.
- `grafana-1c/grafana/dashboards/dlp-dashboard.json`
  DLP Grafana dashboard.

### 2.10 Telegram bot для операторского контура

Развёртывание:

- `ansible/deploy_tsj_guardian_bot_proxmox.yml`

Назначение:

- внешняя проверка AW-Rus + DLP;
- удалённый heal некоторых сценариев;
- контроль доступности worktime/DLP контура извне.

## 3. Какие данные реально собираются

| Канал | Компонент | Bucket | Содержимое |
|---|---|---|---|
| Clipboard | `dlp-endpoint-signals-collector.ps1` | `aw-dlp-endpoint-signals_<host>`, `aw-dlp-incidents_<host>` | hash, length, signal, rule hit, severity, action |
| USB | `dlp-endpoint-signals-collector.ps1` | `aw-dlp-endpoint-signals_<host>`, `aw-dlp-incidents_<host>` | drive letter, volume, signal, enforcement status |
| Print | `dlp-endpoint-signals-collector.ps1` | `aw-dlp-endpoint-signals_<host>`, `aw-dlp-incidents_<host>` | printer, owner, document name, signal, enforcement status |
| Browser domains | `browser-domains-native-collector.ps1` | `aw-watcher-web-*_<host>`, `aw-detmir-web-category_<host>`, `aw-dlp-incidents_<host>` | domain, category, matched policy |
| Email outbound | `email-outbound-collector.ps1` | `aw-email-monitor_<host>`, `aw-dlp-incidents_<host>` | sender/recipient metadata, subject/transport metadata, matched rule |
| File operations | `file-operations-collector.ps1` | `aw-file-operations_<host>` | operation, file path, old path, extension, archive hint |
| RDP sessions | `worktime-session-collector.ps1` | `aw-worktime-sessions_<host>` | username, session id, state, active flag |
| Manual review | `aw-ru-patch.js` | `aw-dlp-review_<host>`, `aw-dlp-rules_<host>` | operator review/suppress/rule decisions |
| Cases | `case_service.py` | SQLite case DB | case metadata, comments, audit, evidence chain |

Дополнительно:

- при DLP-инциденте система может сохранять screenshot artifact;
- OCR применяется к screenshot-артефактам на сервере, не к постоянному видео/потоку;
- compliance и SIEM работают по уже сформированным `aw-dlp-incidents_*`.

## 4. Что система не делает постоянно

- не пишет постоянную запись экрана;
- не делает screenshot по таймеру для обычной активности;
- не реализует встроенную LDAP/SSO/RBAC-аутентификацию внутри policy/case API;
- не подписывает policy bundle криптографически;
- не шифрует AW bucket contents на уровне приложения.

## 5. Основные потоки данных

### 5.1 Endpoint DLP flow

1. Windows collector получает локальное событие.
2. Загружает локальную или серверную DLP policy.
3. Вычисляет match по локальным правилам.
4. При необходимости применяет enforcement.
5. Отправляет heartbeat/event в AW API.
6. При совпадении правила публикует `aw-dlp-incidents_<host>`.
7. При включённом `incidentCapture` сохраняет screenshot artifact metadata.

### 5.2 Policy flow

1. Администратор создаёт/обновляет policy через Policy Engine API.
2. Политика хранится в SQLite с versioning и audit trail.
3. Endpoint в `server` mode делает:
   - `GET /api/0/dlp/policies/active`
   - `GET /api/0/dlp/policies/agents/{agent_id}/desired`
   - `POST /api/0/dlp/policies/agents/{agent_id}/heartbeat`
4. Endpoint кэширует последнюю валидную policy локально.
5. При недоступности сервера используется cached/local fallback.

### 5.3 Review / investigation flow

1. Оператор открывает `#/buckets/aw-dlp-endpoint-signals_<HOST>`.
2. `aw-ru-patch.js` добавляет DLP review/rules центр.
3. Оператор создаёт review/rule запись.
4. UI сохраняет решение в `aw-dlp-review_<host>` или `aw-dlp-rules_<host>`.
5. Из DLP review можно создать кейс расследования.
6. Case Management сохраняет кейс, комментарии и evidence chain.

### 5.4 SIEM / SOAR flow

1. Серверные integrations читают новые события из `aw-dlp-incidents_*`.
2. В зависимости от конфигурации выполняется:
   - CEF export;
   - webhook notification;
   - syslog forwarding.
3. Состояние последнего обработанного `id` хранится локально в state files.

### 5.5 Compliance flow

1. `report-scheduler.timer` запускает генерацию monthly report.
2. `report_generator.py` агрегирует `aw-dlp-incidents_*`.
3. Формируются:
   - HTML отчёт;
   - JSON metadata.

## 6. Реализованные API, службы и порты

### 6.1 HTTP API

| Сервис | Порт | Назначение |
|---|---:|---|
| ActivityWatch API | `5600` | основной API buckets/events/settings |
| Policy Engine | `5601` | централизованная политика DLP |
| Case Management | `5602` | кейсы расследования |
| Worktime API | `5610` | отчёты worktime CSV/JSON |

### 6.2 Systemd units

Критичные сервисы:

- `activitywatch-server`
- `aw-dlp-policy-engine.service`
- `aw-dlp-case-management.service`
- `aw-worktime-api.service`

Критичные timers/services:

- `aw-worktime-ui-bridge.timer`
- `aw-worktime-autoheal.timer`
- `activitywatch-dlp-aggregator.timer`
- `aw-dlp-report-scheduler.timer`
- `aw-dlp-cef-exporter.timer`
- `aw-dlp-webhook-sender.timer`
- `aw-dlp-syslog-forwarder.timer`
- `aw-dlp-ioc-refresh.timer`

## 7. DLP policy engine: реализованный профиль

Policy Engine поддерживает:

- CRUD политик;
- status workflow: `draft -> pending_approval -> approved -> deployed`;
- versioning;
- rollback активной политики;
- audit trail;
- agent heartbeat/desired synchronization.

Ключевые endpoints:

- `GET /healthz`
- `GET/POST/PUT/DELETE /api/0/dlp/policies`
- `GET /api/0/dlp/policies/active`
- `GET /api/0/dlp/policies/active/version`
- `POST /api/0/dlp/policies/{id}/submit`
- `POST /api/0/dlp/policies/{id}/approve`
- `POST /api/0/dlp/policies/{id}/draft`
- `POST /api/0/dlp/policies/{id}/activate`
- `POST /api/0/dlp/policies/rollback`
- `GET /api/0/dlp/policies/audit`
- `GET /api/0/dlp/policies/{id}/audit`
- `POST /api/0/dlp/policies/agents/{agent_id}/heartbeat`
- `GET /api/0/dlp/policies/agents/{agent_id}/desired`

Текущая модель доверия:

- встроенной аутентификации нет;
- защита предполагается сетевой сегментацией, приватным доступом и эксплуатационным контролем.

## 8. Endpoint policy model

Примерные секции политики:

- `defaults`
- `rules`
- `endpoint.clipboard[]`
- `endpoint.usb[]`
- `endpoint.print[]`
- `contentAnalysis.dictionaryPack`
- `contentAnalysis.regexPack`
- `contentAnalysis.ocrEnabled`

Поддерживаемые параметры правил:

- `enabled`
- `cooldownSeconds`
- `action`
- `severity`
- `message`
- `regexPatterns`
- `documentRegex`
- `minLength`
- `dictionaryPack`
- `regexPack`
- `ocrEnabled`

## 9. Enforcement: что реально блокируется

Поддержаны активные действия `action="block"`:

- `clipboard`
  Очистка clipboard.
- `usb`
  Перевод USB media в `read-only`.
- `print`
  Отмена print job.

При enforcement:

- событие всё равно публикуется как инцидент;
- в payload указывается `enforced=true|false`;
- пользователю показывается Windows notification.

Ограничения enforcement:

- для `USB` и части `print` нужны повышенные права;
- при недостатке прав событие будет зафиксировано, но блокировка может не сработать.

## 10. Advanced Content Analysis

Реализовано:

- словари ПДн;
- checksum validation;
- regex packs;
- OCR по screenshot artifact;
- server-side analyzer CLI/module.

Сценарий использования:

1. Endpoint rule указывает `dictionaryPack` и/или `regexPack`.
2. Collector применяет локальный расширенный анализ текста.
3. При наличии screenshot/OCR серверный анализатор может дополнительно разбирать артефакт.

Критичный нюанс:

- OCR и dictionary/regex анализ повышают чувствительность собираемых данных;
- screenshot artifacts и распознанный текст должны рассматриваться как sensitive evidence.

## 11. Case Management

Реализовано:

- создание кейса;
- обновление кейса;
- комментарии;
- audit trail;
- evidence chain с `sha256`.

Ключевые endpoints:

- `GET /health`
- `POST /api/0/dlp/cases`
- `GET /api/0/dlp/cases`
- `GET /api/0/dlp/cases/{id}`
- `PATCH /api/0/dlp/cases/{id}`
- `POST /api/0/dlp/cases/{id}/comments`
- `GET /api/0/dlp/cases/{id}/comments`

Особенность текущей реализации:

- `case_service.py` сейчас использует permissive CORS, включая `"*"`;
- для hardened production это должно быть сужено до контролируемых origin.

## 12. WebUI overlay и операторский workflow

В `aw-ru-patch.js` реализованы:

- русифицированная навигация;
- DLP bucket deep-links;
- DLP review center;
- DLP rules manager;
- создание кейса из review;
- отдельная секция DLP incidents.

Операторские служебные buckets:

- `aw-dlp-review_<host>`
- `aw-dlp-rules_<host>`

Это не источники endpoint-телеметрии, а слой операторской классификации и suppression.

## 13. SIEM / SOAR

Реализованы три потока:

- CEF export;
- webhook notifications;
- syslog forwarding.

Источник всегда один: `aw-dlp-incidents_*`.

Текущая модель:

- state хранится локально на сервере;
- обработка идёт по event `id`;
- доставка зависит от сетевой доступности получателя и конфигурации transport.

ИБ-нюанс:

- безопасность отправки определяется настройкой конкретного канала;
- если syslog/webhook настроены без TLS или во внешний контур, это уже операционный риск, а не защита приложения.

## 14. Compliance reporting

Реализованы профили:

- `152-fz`
- `pci-dss`

Результат:

- HTML report;
- JSON metadata.

Отчёт агрегирует:

- общее число инцидентов;
- распределение по severity;
- распределение по host;
- распределение по channel.

## 15. IOC enrichment через Hayabusa / Sigma

Реализован вспомогательный pipeline:

- разбор Sigma/YAML правил;
- извлечение IOC-полей;
- выгрузка в `json/csv/sql`.

Извлекаемые типы:

- `Image|endswith`
- `CommandLine|contains`
- `OriginalFileName`
- `Hashes|SHA256`

Назначение:

- preload blacklist/indicator данных для DLP и смежной аналитики.

## 16. Health-check, autoheal и эксплуатационная устойчивость

### 16.1 Базовые проверки

- `/usr/local/bin/aw-health-check`
- `/usr/local/bin/dlp-health-check`

Проверяется:

- HTTP-доступность сервисов;
- состояние systemd units/timers;
- свежесть bucket-ов;
- наличие transport self-test metrics;
- наличие compliance artifacts.

### 16.2 File-operations health model

Текущая логика `dlp-health-check.py` специально учитывает production-реальность:

- `aw-file-operations_*` обязателен только для managed host с реально активным `aw-worktime-sessions_*`;
- исторические или unmanaged bucket-ы не считаются аварией;
- это устраняет ложные alarms при отсутствии активной интерактивной RDP-сессии.

### 16.3 Worktime autoheal

`aw-worktime-autoheal.sh`:

- проверяет доступность worktime report endpoint;
- при необходимости перезапускает `aw-worktime-api.service`;
- нормализует `aw-watcher-window_*` и `aw-watcher-afk_*` из `aw-worktime-sessions_*`;
- выполняет hard normalization повреждённых bucket-ов.

### 16.4 Manual recovery

`scripts/diag_and_manual_restart.sh`:

- запускает health-check;
- при fail рестартует серверные компоненты;
- опционально инициирует Windows recovery/launch tasks;
- может выполнить seed self-test событий для восстановления freshness.

## 17. Деплой и управление изменениями

Ключевые playbook:

- `ansible/deploy_aw_server.yml`
- `ansible/deploy_aw_windows.yml`
- `ansible/deploy_dlp_full_stack.yml`
- `ansible/deploy_tsj_guardian_bot_proxmox.yml`

Роли:

- `ansible/roles/dlp-policy-engine`
- `ansible/roles/dlp-content-analysis`
- `ansible/roles/dlp-integrations`
- `ansible/roles/dlp-case-management`
- `ansible/roles/dlp-compliance`

Post-deploy gates:

- `aw-health-check`
- `dlp-health-check --json`

## 18. Минимальный эксплуатационный набор для ИБ

Для регулярной проверки достаточно:

```bash
/usr/local/bin/aw-health-check
/usr/local/bin/dlp-health-check --json
python3 scripts/dlp-admin-cli.py health check
python3 scripts/dlp-admin-cli.py policies active
python3 scripts/dlp-admin-cli.py incidents list --since-hours 24 --limit 50
python3 scripts/dlp-admin-cli.py cases list --limit 50
```

## 19. Ограничения и остаточные риски

Критичные ограничения текущей реализации:

- нет встроенной auth/RBAC в Policy Engine;
- нет встроенной auth/RBAC в Case Management;
- Case Management использует permissive CORS;
- policy distribution не подписывается криптографически;
- данные в AW buckets и локальных SQLite DB не шифруются приложением;
- screenshot/OCR artifacts содержат чувствительные данные и требуют отдельного режима хранения/ретенции;
- эффективность enforcement зависит от запуска collector под достаточными правами;
- webhook/syslog/CEF transport security зависит от конфигурации канала;
- AGENT heartbeat state в Policy Engine хранится в памяти процесса и не является полноценным durable registry;
- manual review buckets являются операторским слоем и не должны трактоваться как первичный доказательный источник без сверки с исходным incident bucket.

## 20. Рекомендации службе ИБ по допуску в production

Перед formal acceptance рекомендуется как минимум:

1. Ограничить доступ к `5601` и `5602` сетевой сегментацией и reverse proxy policy.
2. Убрать wildcard CORS из `Case Management`.
3. Определить политику хранения и удаления screenshot/OCR artifacts.
4. Формализовать список доверенных операторов review/case workflow.
5. Включить TLS или закрытый management network для syslog/webhook/CEF маршрутов.
6. Зафиксировать backup/restore для:
   - `dlp-policy-engine.sqlite`
   - `cases.db`
   - compliance reports
   - IOC artifacts
7. Прописать регламент ручной верификации после каждого `deploy_aw_server.yml` и `deploy_aw_windows.yml`.

## 21. Связанные документы

- `docs/dlp-policy-engine.md`
- `docs/dlp-integrations.md`
- `docs/dlp-enforcement.md`
- `docs/dlp-aggregator.md`
- `docs/email-outbound-collector.md`
- `docs/windows/deployment.md`
- `docs/windows/validation.md`
- `docs/runbook.md`
- `docs/worktime_aql_detmir.md`

