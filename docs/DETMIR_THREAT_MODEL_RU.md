# DetMir: операционная модель угроз

Дата фиксации: `2026-06-03`

Статус: рабочая модель угроз `v0.1` для текущего production-контура
`DetMir/AWatch-rus`.

Документ фиксирует фактически используемую модель угроз для эксплуатации,
аудита, развития продукта и подготовки к возможной регистрации как
отечественного программного продукта.

## 1. Правообладание и позиционирование

На момент фиксации правообладателем продукта заявлен Игорь, владелец текущего
репозитория и production-контура DetMir/AWatch-rus.

Рабочее позиционирование продукта:

> DetMir/AWatch-rus - отечественная платформа операционного контроля,
> технического аудита, мониторинга действий пользователей, расследования
> инцидентов и автоматизации реагирования.

Текущую модель угроз не следует трактовать как:

- формальную модель угроз ИСПДн, ГИС или КИИ;
- сертифицированную модель угроз ФСТЭК;
- заявление соответствия классу сертифицированных DLP-систем;
- заявление соответствия классу SIEM, EDR или XDR;
- заверенную модель нарушителя для средства защиты информации.

Для реестра российского ПО и коммерческого позиционирования безопаснее
использовать классы:

- система автоматизации ИТ-эксплуатации;
- платформа технического аудита;
- платформа операционного контроля;
- система мониторинга и контроля регламентов.

Функции DLP, ИБ-инцидентов, evidence и Hayabusa в этой модели являются
прикладными возможностями платформы, а не основанием заявлять продукт как
сертифицированную СЗИ.

## 2. Границы модели

В модель входят:

- Windows/RDP endpoint collectors;
- ActivityWatch/AW-rus server;
- Rust-сервисы DetMir на Proxmox и AW server;
- операторский портал DetMir;
- Grafana/Influx/SQLite/warehouse/reporting layer;
- Telegram bot как постоянный Python runtime;
- DLP incident/evidence pipeline;
- Hayabusa/offline DFIR enrichment;
- 1C/file analytics контур как смежный аналитический слой;
- Ansible/systemd/scheduled-task deployment и recovery path.

В модель не входят:

- pfSense как объект активной доработки в рамках текущей фазы;
- внешняя юридическая аттестация;
- криптографическая защита канала сверх уже настроенной инфраструктуры;
- защита от администратора, полностью контролирующего endpoint или сервер;
- гарантия неизменности данных без внешнего WORM/подписанного хранилища.

## 3. Защищаемые активы

Основные активы:

| Актив | Почему важен |
|---|---|
| ActivityWatch buckets | Первичная телеметрия активности, worktime, browser, DLP и session-событий. |
| DLP incidents | Первичная фиксация событий, требующих проверки оператором или ИБ. |
| Evidence/screenshots | Подтверждения инцидентов, потенциально содержащие чувствительные данные. |
| Evidence audit | След просмотра, загрузки и скачивания доказательств. |
| DLP warehouse SQLite | Нормализованный источник для портала, Grafana и reporting. |
| Grafana dashboards | Управленческая и ИБ-витрина состояния контура. |
| 1C/file analytics | Деловая аналитика и признаки аномалий по компаниям/файлам. |
| Tokens/secrets | Доступ к upload API, SSH/WinRM, Grafana, gateway, Ansible runtime. |
| Systemd timers/services | Автономная работа без зависимости от ноутбука. |
| Runbooks/backups/rollback artifacts | Восстановление после ошибок deploy или runtime-регрессий. |

## 4. Доверенные зоны

Рабочая модель разделяет контур на зоны:

| Зона | Пример | Уровень доверия |
|---|---|---|
| Endpoint/RDP | `192.168.100.18` | Доверенный источник сигналов, но допускается риск локального вмешательства. |
| AW server | `10.10.10.13` | Основной trusted data/control plane для AW/DLP/worktime. |
| Proxmox/operator | `10.10.10.2` | Операторский gateway, portal, Telegram, automation entrypoint. |
| Grafana/data | Grafana/Influx/ClickHouse | Витрина и аналитика, не первичный источник доказательств. |
| Laptop/operator shell | рабочая станция владельца | Удобный admin-клиент, но не обязательный runtime. |
| External access | `dm.iri1968.dpdns.org` | Доступ только через gateway/auth/reverse proxy. |

Ключевой принцип: production-контур должен работать автономно на серверах.
Ноутбук не является обязательной частью runtime.

## 5. Модель нарушителя

Модель предполагает следующие типы нарушителей и отказов.

### 5.1 Обычный пользователь

Возможности:

- работает в RDP/Windows-сессии;
- использует браузер, файлы, печать, clipboard, email;
- может случайно или намеренно создать DLP-событие.

Цель контроля:

- обнаружить подозрительное действие;
- сохранить событие и доступные доказательства;
- показать оператору состояние в портале/Grafana;
- не создавать ложную аварию при нормальном отсутствии событий.

### 5.2 Внутренний нарушитель с локальными правами

Возможности:

- пытается остановить collectors;
- удаляет локальные artifacts;
- меняет локальные конфиги;
- пытается обойти endpoint monitoring.

Текущие ограничения:

- если у нарушителя полный локальный admin-контроль, endpoint не считается
  криптографически доверенным источником;
- текущая система фиксирует и восстанавливает runtime, но не является EDR.

### 5.3 Оператор или администратор, совершивший ошибку

Возможности:

- запускает неверный deploy;
- меняет переменные окружения;
- ломает systemd unit, scheduled task, Grafana datasource или policy;
- случайно открывает/удаляет evidence.

Контрмеры:

- runbook-first workflow;
- backup-first перед рискованными изменениями;
- read-only parity перед replacement;
- dry-run для опасных операций;
- audit evidence view/download/upload;
- rollback artifacts.

### 5.4 Внешний атакующий с доступом к gateway/VPN/порталу

Возможности:

- пытается открыть portal/Grafana/AW/evidence API;
- пробует прямые пути к файлам доказательств;
- пытается загрузить fake evidence;
- пытается получить внутренние endpoints через reverse proxy.

Контрмеры:

- gateway authentication;
- evidence routes only by opaque `evidence_id`;
- запрет raw path serving;
- canonical path/root allowlist;
- Bearer token для upload API;
- `403` при upload без токена;
- SHA-256 validation и file magic check;
- max-size limit;
- atomic evidence write.

### 5.5 Вредоносное ПО или скрипт на endpoint

Возможности:

- генерирует шумовые события;
- пытается подложить screenshot/artifact;
- мешает scheduled tasks;
- вызывает сетевые/HTTP ошибки.

Контрмеры:

- upload state tracking;
- SHA-256 contract между event metadata и artifact bytes;
- DLP health gates;
- Windows scheduled task recovery;
- AW SLO/current sample monitoring;
- Rust health/status helpers.

### 5.6 Отказ компонента

Отказы считаются частью операционной модели угроз:

- ActivityWatch API недоступен;
- Rocket/HTTP keep-alive закрывает соединение до конца ответа;
- Grafana datasource stale/empty;
- Influx/SQLite/reporting дает неполные данные;
- sync task не доставляет evidence;
- systemd timer/service падает;
- старые false samples искажают SLO.

Контрмеры:

- `detmir-status`, `detmir-check`, `dlp-health-check`;
- SLO summary/current sample;
- systemd failed-unit checks;
- browser smoke;
- Grafana data freshness check;
- controlled reset/trim только после анализа причины;
- автономные timers на серверах.

## 6. Основные угрозы

| ID | Угроза | Текущий статус контроля |
|---|---|---|
| T01 | Потеря или остановка endpoint collectors | Health/recovery/scheduled tasks, но local admin остается residual risk. |
| T02 | Ложные DLP warnings из-за старого состояния | Delta/baseline logic, SLO correction, health semantics. |
| T03 | Несанкционированный просмотр evidence | Gateway auth, opaque id, audit view/download. |
| T04 | Подмена screenshot/evidence | SHA-256 contract, magic check, upload token, atomic write. |
| T05 | Прямая выдача файлов по path traversal | Canonical path/root allowlist, no raw path route. |
| T06 | Утечка upload token/secrets | Token outside git, no secret printing policy, journald cleanup after incidents. |
| T07 | Поломка Grafana dashboards после миграции | Grafana check, datasource health, portal smoke. |
| T08 | Автоагент выполняет опасное действие | Allowlist/dry-run/read-only parity, pfSense frozen/no-touch. |
| T09 | Портал показывает неверный статус | DetMir status backend, data freshness checks, smoke tests. |
| T10 | Ноутбук становится runtime-зависимостью | systemd/scheduled tasks on servers, autonomous sync/portal/services. |
| T11 | Evidence содержит чувствительные данные | Нужна формальная retention/access/redaction policy. |
| T12 | Злоупотребление админскими правами | Полностью не закрыто без отдельного PAM/RBAC/WORM/EDR слоя. |

## 7. Реализованные контрмеры

На момент фиксации реализованы и проверены:

- Rust-first production helpers для `detmir-status`, `detmir-check`,
  `detmir-dlp`, `detmir-auto`, DLP/worktime/status paths;
- Telegram runtime оставлен на Python, Rust используется как backend helper;
- evidence-only API на AW server;
- безопасные screenshot routes через portal/gateway;
- upload API с Bearer token;
- base64 decode server-side;
- PNG/JPEG magic validation;
- max-size enforcement;
- SHA-256 validation;
- atomic write в evidence storage;
- audit records для upload/view/download;
- Windows scheduled task `ActivityWatch DLP Evidence Sync` каждые 5 минут;
- external Playwright smoke для портала/evidence/Grafana routes;
- Grafana актуальность и datasource health checks;
- rollback-critical backups;
- запрет трогать pfSense без отдельной явной команды.

## 8. Остаточные риски

Остаточные риски, которые не надо скрывать:

- нет формальной модели угроз ИСПДн/ГИС/КИИ;
- нет ФСТЭК-сертификации;
- нет нативного полного RBAC в DLP policy/case/evidence workflow;
- screenshot artifacts могут содержать персональные, коммерческие или
  чувствительные данные;
- retention/redaction/access policy для evidence требует отдельной фиксации;
- endpoint с локальным admin не является полностью доверенным;
- policy distribution пока не является криптографически подписанной;
- SQLite/AW buckets не шифруются приложением;
- внешние syslog/webhook/CEF каналы зависят от конфигурации транспорта;
- Grafana остается витриной, а не первичным источником доказательств;
- юридическая доказательная сила evidence требует отдельного регламента.

## 9. Требования к эксплуатации

Для текущей модели обязательны:

1. Не хранить секреты в git.
2. Не печатать токены в stdout, journald, reports или Telegram.
3. Держать upload token только на AW server и Windows endpoint.
4. Доступ к portal/Grafana/evidence отдавать только через контролируемый gateway.
5. Не открывать внутренние сервисные порты наружу без отдельного решения.
6. Перед risky changes делать backup и фиксировать rollback path.
7. Для replacement scripts использовать read-only parity и shadow validation.
8. Проверять Grafana не только по HTTP login, но и по datasource/data freshness.
9. После DLP/evidence изменений выполнять synthetic incident smoke и cleanup.
10. Не трогать pfSense в рамках app-level работ без отдельной явной команды.

## 10. Roadmap усиления модели

Ближайшие улучшения:

1. Зафиксировать evidence retention/access policy.
2. Добавить role-aware views в DetMir Portal.
3. Разделить operator/owner/security auditor роли.
4. Добавить immutable audit export для evidence actions.
5. Ввести policy signing или хотя бы signed policy bundle checksum.
6. Добавить периодический evidence integrity scan.
7. Описать data classification для screenshots/OCR/file analytics.
8. Подготовить отдельный пакет документов для реестра российского ПО.
9. Подготовить отдельный юридический контур, если понадобится заявлять СЗИ.

## 11. Практическая формулировка

Для внутренних и внешних описаний использовать:

> DetMir/AWatch-rus использует рабочую операционную модель угроз для платформы
> технического аудита и операционного контроля. Модель покрывает сбор
> активности, DLP-события, доказательства инцидентов, мониторинг состояния
> сервисов, ошибки операторов, отказ компонентов и базовые сценарии
> несанкционированного доступа к evidence/portal/API. Модель не является
> сертифицированной моделью угроз ФСТЭК и не заявляет продукт как
> сертифицированное средство защиты информации.

## 12. Связанные документы

- `docs/DETMIR_RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md`
- `docs/DETMIR_UNIFIED_OPERATING_MODEL_RU.md`
- `docs/DETMIR_PORTAL_GUI_PLAN_RU.md`
- `docs/dlp-security-functional-spec-ru.md`
- `docs/dlp-gap-analysis.md`
- `docs/dlp-production-plan-windows-10-19.md`
- `docs/security-analytics-stack-v1.md`
- `docs/runbook.md`
- `adk-rust/RUNBOOK.md`
- `.ai/runtime/detmir-current-session.md`
