# Чек-лист пилотного внедрения DetMir

Дата фиксации: `2026-06-03`.

Чек-лист предназначен для внедрения у заказчика. Публичная версия использует
placeholders и не содержит live infrastructure values.

## 1. До внедрения

| Проверка | Статус | Комментарий |
|---|---|---|
| Определен правообладатель/исполнитель | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Согласован scope пилота | `<OK/WARN/FAIL>` | Workforce, readiness, DLP-lite, evidence, Grafana. |
| Согласованы роли доступа | `<OK/WARN/FAIL>` | Владелец, руководитель, оператор, ИБ, администратор. |
| Согласованы данные, которые собираются | `<OK/WARN/FAIL>` | Activity, worktime, DLP-lite metadata, evidence policy. |
| Согласованы правила уведомления сотрудников | `<OK/WARN/FAIL>` | Локальные нормативные документы заказчика. |
| Согласованы retention и backup | `<OK/WARN/FAIL>` | Events, evidence, readiness bundles, logs. |
| pfSense/network enforcement исключен или отдельно согласован | `<OK/WARN/FAIL>` | По умолчанию read-only/no-touch. |

## 2. Инфраструктура

| Проверка | Статус | Комментарий |
|---|---|---|
| Выделен server host/VM | `<OK/WARN/FAIL>` | `<AW_SERVER_HOST>` |
| Выделен portal/gateway host | `<OK/WARN/FAIL>` | `<GATEWAY_HOST>` |
| Определен Grafana/Prometheus profile | `<OK/WARN/FAIL>` | `<GRAFANA_HOST>` |
| Подготовлены endpoint hosts | `<OK/WARN/FAIL>` | `HOST-EXAMPLE` вместо live hostnames в docs. |
| DNS/TLS/auth gateway согласованы | `<OK/WARN/FAIL>` | Без публикации secrets в Git. |
| Backup path согласован | `<OK/WARN/FAIL>` | SQLite/evidence/config backups. |

## 3. Установка

| Проверка | Статус | Комментарий |
|---|---|---|
| Репозиторий checkout на clean VM | `<OK/WARN/FAIL>` | Проверенный tag/release. |
| SBOM/release assets скачаны | `<OK/WARN/FAIL>` | CycloneDX/SPDX, SHA256SUMS, signature. |
| Release assets проверены | `<OK/WARN/FAIL>` | `scripts/verify_release_assets.sh`. |
| Private inventory создан вне Git | `<OK/WARN/FAIL>` | `ansible/inventory.ini` ignored. |
| Secrets/env созданы вне Git | `<OK/WARN/FAIL>` | `private-config/*.env`, systemd env. |
| DetMir services deployed | `<OK/WARN/FAIL>` | Только project apps, не platform layer. |
| Endpoint collectors deployed | `<OK/WARN/FAIL>` | Windows/RDP profile. |

## 4. Первичная проверка

| Проверка | Статус | Комментарий |
|---|---|---|
| Portal login | `<OK/WARN/FAIL>` | Auth gateway работает. |
| Readiness bundle | `<OK/WARN/FAIL>` | OK/WARN/FAIL, checksum, signature. |
| Prometheus alerts | `<OK/WARN/FAIL>` | readiness/signature alerts loaded. |
| Grafana dashboards | `<OK/WARN/FAIL>` | Нет query errors/no data на ключевых panels. |
| ActivityWatch freshness | `<OK/WARN/FAIL>` | Buckets fresh. |
| Workforce report | `<OK/WARN/FAIL>` | Индекс активности и role weights. |
| DLP-lite test incident | `<OK/WARN/FAIL>` | Synthetic USB/print/clipboard/file event. |
| Evidence preview/download/audit | `<OK/WARN/FAIL>` | Opaque ID, no raw path. |

## 5. Пилотная эксплуатация

| Проверка | Статус | Комментарий |
|---|---|---|
| Ежедневный readiness act формируется | `<OK/WARN/FAIL>` | Timer/retention OK. |
| Еженедельный управленческий отчет сформирован | `<OK/WARN/FAIL>` | Owner/manager view accepted. |
| Incident workflow пройден | `<OK/WARN/FAIL>` | Ack/assign/evidence/export. |
| Роли и веса приложений согласованы | `<OK/WARN/FAIL>` | Нет спорных default_weight без review. |
| Pilot acceptance act заполнен | `<OK/WARN/FAIL>` | `docs/CUSTOMER_PILOT_ACCEPTANCE_RU.md`. |

## 6. Критерий завершения пилота

Пилот можно закрывать как успешный, если:

- signed readiness bundle проверяется;
- портал доступен целевым ролям;
- Workforce analytics понятна заказчику;
- DLP-lite/evidence workflow воспроизводим;
- Grafana dashboards актуальны;
- нет live secrets/private paths в передаваемых materials;
- заказчик подписал акт приемки или список доработок.
