# Модель безопасности DetMir/AWatch-rus

Дата фиксации: `2026-06-03`.

Документ описывает роли, границы доверия, собираемые данные, хранение и доступ.
Это не сертифицированная модель угроз ФСТЭК и не юридическое заключение.

## 1. Роли

| Роль | Назначение | Доступ |
|---|---|---|
| Владелец бизнеса | Смотрит управленческие KPI и итоговые отчеты | Portal owner/report views, агрегаты без технических деталей по умолчанию. |
| Руководитель подразделения | Анализирует загрузку и активность команды | Workforce dashboards, role/app weights, drill-down в рамках подразделения. |
| Оператор DetMir | Ежедневная эксплуатация и triage | Portal operator views, readiness, incidents, links, acknowledgement workflow. |
| Администратор системы | Установка, обновления, сервисы, backup/restore | Server shell/systemd/Ansible, конфигурация и runtime paths. |
| Специалист ИБ | Разбор DLP-lite/ИБ событий | Incident/evidence views, Grafana security dashboards, export reports. |
| Разработчик/maintainer | Развитие продукта | Source repo, release tooling, CI, SBOM, docs. Не должен иметь customer secrets в Git. |

## 2. Границы доверия

| Граница | Внутри доверенной зоны | Снаружи/ниже доверие | Контроль |
|---|---|---|---|
| Endpoint -> AW server | Windows/RDP collectors | Пользовательская сессия, локальные процессы | Transport queue, heartbeat, server-side validation. |
| AW server -> Portal/Grafana | Нормализованные события и агрегаты | Browser пользователя портала | Auth gateway, read-only API views, no raw path serving. |
| Evidence storage -> Evidence API | Files under allowlisted storage area | Raw path/request input | Opaque IDs, canonical path and storage-root allowlist, magic/size/hash checks. |
| Runtime config -> Public repo | Private inventory/env/systemd secrets | GitHub/public release | `.gitignore`, placeholders, public hygiene scan. |
| Readiness bundle -> Operator | Signed checksums and public key | Tampered artifacts | SHA-256, detached signature, fingerprint in runbook. |
| Optional network perimeter | pfSense/firewall context | Enforcement actions | Read-only by default; mutation only by separate change request. |

## 3. Какие данные собираются

| Категория | Примеры | Назначение | Ограничение |
|---|---|---|---|
| Activity telemetry | window/app/activity/AFK/session events | Workforce analytics и техаудит | Не трактовать как абсолютную оценку полезности без role weights. |
| Worktime/session telemetry | RDP/session presence, active seconds | Индекс активности, загрузка, тренды | Proxy-метрика, требует локальных регламентов. |
| DLP-lite signals | clipboard/USB/print/file/email/browser metadata | Фиксация возможных ИБ-инцидентов | Не заявлять как enterprise DLP. |
| Evidence | screenshots/metadata/hashes при включении | Подтверждение и разбор событий | Не публиковать customer evidence в Git/release. |
| Service health | systemd, datasource, readiness, SLO | Эксплуатационная готовность | Не содержит секретов при корректной настройке. |
| 1C/business telemetry | file/business event aggregates | Управленческая аналитика | Отдельный профиль доступа и обезличивания. |

## 4. Где хранятся данные

| Хранилище | Данные | Доступ |
|---|---|---|
| ActivityWatch buckets | Endpoint/worktime/DLP events | AW server/API, service users. |
| SQLite state DB/files | Cases, local state, readiness, helpers | Local service users/admin. |
| Evidence directory | Screenshots/artifacts | Evidence API через opaque IDs; direct FS только администратору. |
| InfluxDB/TSDB | Metrics/time-series | Grafana/Prometheus profile. |
| ClickHouse | 1C/business analytics, если включено | Отдельный business-data contour. |
| GitHub release assets | SBOM, checksums, docs/screenshots demo | Только обезличенные public artifacts. |

## 5. Доступ и контроль

- Public repository не должен содержать live inventory, domains, private IPs,
  secrets, runtime DB, customer evidence или реальные forensic paths.
- Portal/gateway должен требовать аутентификацию для внешнего доступа.
- Evidence API не должен отдавать raw filesystem paths.
- Readiness bundle должен иметь checksum и detached signature.
- Prometheus/Grafana должны сигнализировать при readiness/signature failure.
- pfSense/network enforcement выключен по умолчанию и не является обязательной
  частью продукта.

## 6. Ограничения модели

DetMir/AWatch-rus в текущем positioning не заявляется как:

- сертифицированная СЗИ;
- enterprise SIEM;
- enterprise DLP;
- EDR/XDR;
- юридически неизменяемое evidence/WORM-хранилище.

Корректная формулировка: платформа операционного контроля, технического аудита,
мониторинга пользовательской активности, аналитики событий и расследования
операционных/ИБ-инцидентов.
