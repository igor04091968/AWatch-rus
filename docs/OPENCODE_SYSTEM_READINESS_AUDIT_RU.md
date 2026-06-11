# Readiness Audit: handover plan vs codebase reality

**Дата:** 2026-06-11
**Метод:** read-only audit всех entrypoints из handover-плана + AGENTS.md + файловая система.
**Правило:** ничего не менять, не деплоить, не коммитить.

---

## Executive Summary

Handover-план (613 строк) в целом соответствует кодовой базе: все ключевые
компоненты существуют. Найдено 5 расхождений между планом и реальностью,
1 missing-позиция (некритична). Первичный вывод про `clickhouse-1c/ai/` был ложным: директория существует.
Inventory.ini содержит **production credentials в открытом виде**. Значения в
этом отчете намеренно не фиксируются; это единственный критический blocker.
Ниже — детальная таблица по каждому слою.

---

## Полная таблица проверки

| # | Слой | Функционал (из handover) | Файл/модуль/команда | Проверка | Статус | Что делать дальше |
|---|------|--------------------------|---------------------|----------|--------|-------------------|
| 1 | **AW server** | `activitywatch-server` на `:5600` | `aw-server/activitywatch-server.service` | Файл существует | OK | — |
| 2 | AW server | WebUI + CORS | `aw-server/` (общий деплой) | Ansible `deploy_aw_server.yml` разворачивает | OK | — |
| 3 | AW server | SQLite не перегружен | `adk-rust/crates/aw-db-maintenance/` | Крейт существует | OK | — |
| 4 | AW server | RU patch v5 | `aw-server/aw-ru-patch.js` → деплоится как `ru-patch-v5.js` | Ansible line 450 маппит aw-ru-patch.js → ru-patch-v5.js | OK | В handover-плане (строка 185) curl проверяет `/js/ru-patch-v5.js` — это корректный URL после деплоя, но файла `ru-patch-v5.js` в репозитории нет, только `aw-ru-patch.js` |
| 5 | AW server | Host sanitize script | `aw-server/aw-host-sanitize.js` | Существует | OK | — |
| 6 | AW server | Worktime panel | `aw-server/aw-worktime-panel.js` | Существует | OK | — |
| 7 | **Windows/RDP** | `deploy-ensemble.ps1` | `windows/deploy-ensemble.ps1` | Существует | OK | — |
| 8 | Windows/RDP | `validate-deployment.ps1` | `windows/validate-deployment.ps1` | Существует | OK | — |
| 9 | Windows/RDP | `deployment-config.json` | Нет в репозитории (runtime-файл на Windows) | Созётся скриптами, упоминается в 120+ местах | OK | Ожидаемое поведение: файл генерируется на хосте |
| 10 | Windows/RDP | Scheduled Tasks `ActivityWatch Launch/Recovery` | `windows/install-collector-guard-service.ps1`, `windows/aw-collector-guard.ps1` | Скрипты существуют | OK | — |
| 11 | Windows/RDP | Rust collector guard (C# service) | `windows/AWatchRusCollectorGuardService.cs` | Существует | OK | — |
| 12 | Windows/RDP | PowerShell fallback | `windows/aw-collector-guard.ps1` | Существует | OK | — |
| 13 | Windows/RDP | Все DLP-коллекторы | `windows/dlp-endpoint-signals-collector.ps1`, `file-operations-collector.ps1`, `browser-domains-native-collector.ps1`, `email-outbound-collector.ps1`, `dlp-policy-client.ps1` | Все существуют | OK | — |
| 14 | Windows/RDP | Worktime session collector | `windows/worktime-session-collector.ps1` | Существует | OK | — |
| 15 | Windows/RDP | Evidence sync | `windows/sync-dlp-evidence-artifacts.ps1` | Существует | OK | — |
| 16 | Windows/RDP | Общий PowerShell module | `windows/ActivityWatch.Windows.Common.psm1` (2667 строк) | Существует | OK | — |
| 17 | Windows/RDP | InnoSetup install kit | `windows/installkit/innosetup/` | Существует c filelist | OK | — |
| 18 | **Worktime API** | `aw-worktime-api` на `:5610` | `adk-rust/crates/worktime-api/` + `aw-server/aw-worktime-api.service` | Крейт + service существует | OK | — |
| 19 | Worktime API | `/health` endpoint | В коде worktime-api | Есть | OK | — |
| 20 | Worktime API | `/reports/worktime/management` | В коде worktime-api | Есть | OK | — |
| 21 | Worktime API | stale cache | В коде worktime-api | Есть | OK | — |
| 22 | Worktime API | Prewarm | `aw-server/aw-worktime-prewarm.sh` + `aw-server/aw-worktime-prewarm.service` + `.timer` | Существует | OK | — |
| 23 | **DLP** | Policy engine Rust | `adk-rust/crates/dlp-policy-engine/` + `aw-server/dlp-policy-engine/dlp-policy-engine.service` | Существует | OK | — |
| 24 | DLP | Case management Rust | `adk-rust/crates/dlp-case-management/` + `aw-server/dlp-case-management/case-service.service` | Существует | OK | — |
| 25 | DLP | Compliance Rust | `adk-rust/crates/dlp-compliance/` + `aw-server/dlp-compliance/report-scheduler.service` | Существует | OK | — |
| 26 | DLP | Content analyzer (Python) | `aw-server/dlp-content-analysis/` | Существует (Python) | OK | В AGENTS.md Python разрешён именно для этого |
| 27 | DLP | DLP health check | `adk-rust/crates/dlp-health-check/` | Существует | OK | — |
| 28 | DLP | DLP Influx exporter | `adk-rust/crates/dlp-influx-exporter/` + `aw-server/aw-dlp-influx-exporter.service` | Существует | OK | — |
| 29 | DLP | DLP admin CLI | `adk-rust/crates/dlp-admin-cli/` | Существует | OK | — |
| 30 | DLP | DLP aggregator | `adk-rust/crates/dlp-aggregator/` | Существует | OK | — |
| 31 | DLP | DLP CEF exporter | `adk-rust/crates/dlp-cef-exporter/` | Существует | OK | — |
| 32 | DLP | DLP webhook sender | `adk-rust/crates/dlp-webhook-sender/` | Существует | OK | — |
| 33 | DLP | DLP syslog forwarder | `adk-rust/crates/dlp-syslog-forwarder/` | Существует | OK | — |
| 34 | **WebUI** | RU patch подключён | Ansible `deploy_aw_server.yml` + `apply_webui_ru_patch.sh` | Работает через Ansible | OK | — |
| 35 | WebUI | Host sanitize подключён | Ansible деплоит `aw-host-sanitize.js` | Есть | OK | — |
| 36 | WebUI | browser cache | Cache-bust через `aw_ru_patch_cache_bust` в Ansible | Есть | OK | — |
| 37 | **Portal** | Порт `:8720` | `detmir-portal/src/main.rs` строка 161: `default_value = "127.0.0.1:8720"` | Совпадает с handover | OK | — |
| 38 | Portal | `/api/health` | `main.rs` строка 1517 | Есть | OK | — |
| 39 | Portal | `/api/reports` | `main.rs` строка 1583 | Есть | OK | — |
| 40 | Portal | Read-only | Заявлено как read-only в коде | OK | OK | — |
| 41 | Portal | Role views | PortalRole enum, role filtering | Есть | OK | — |
| 42 | Portal | HTML static SPA | `detmir-portal/src/static/index.html` + `app.js` | Есть | OK | — |
| 43 | Portal | Документация | `docs/PORTAL_RU.md` | Существует | OK | — |
| 44 | **Grafana/Influx** | Dashboards в `grafana/` | `grafana/detmir-aw-main-dashboard.json`, `detmir-rdp-user-activity-dashboard.json`, `detmir-dlp-security-dashboard.json`, `dlp-dashboard.json`, `detmir-dlp-management-dashboard.json`, `pfsense-loki-dashboard.json` | 6 файлов | OK | Handover план говорит `grafana/`, но реальность — плоские JSON без provisioning-структуры |
| 45 | Grafana/Influx | Influx exporters | `aw-server/aw-worktime-influx-exporter.service` + `aw-server/aw-dlp-influx-exporter.service` + соответствующие Rust крейты | Существуют | OK | — |
| 46 | Grafana/Influx | `deploy_grafana_check.yml` | `ansible/deploy_grafana_check.yml` | Существует | OK | — |
| 47 | Grafana/Influx | `deploy_grafana_dashboards.yml` | `ansible/deploy_grafana_dashboards.yml` | Существует (НЕ упомянут в handover) | NEEDS_VERIFICATION | Handover не упоминает этот playbook, но он существует |
| 48 | Grafana/Influx | `grafana-1c/` отдельный стек | `grafana-1c/docker-compose.yml`, `grafana-1c/grafana/dashboards/*.json` | Существует для 1C MSSQL/Postgres | OK | Handover не выделяет отдельный стек grafana-1c |
| 49 | **ClickHouse/1C** | Docker Compose | `clickhouse-1c/docker-compose.yml` | Существует | OK | — |
| 50 | ClickHouse/1C | Init SQL (6 файлов) | `clickhouse-1c/clickhouse/init/00_database.sql` – `05_financial_reporting.sql` | Все 6 существуют | OK | — |
| 51 | ClickHouse/1C | Detection SQL | `clickhouse-1c/detections/insert_detections.sql`, `build_entity_timeline.sql`, `open_cases_from_detections.sql` | Все 3 существуют | OK | — |
| 52 | ClickHouse/1C | ETL Python | `clickhouse-1c/etl/*.py` | 6 Python-файлов | OK | — |
| 53 | ClickHouse/1C | Grafana 1C dashboards | `clickhouse-1c/grafana/provisioning/dashboards/files/*.json` | 10 dashboard JSON | OK | — |
| 54 | ClickHouse/1C | Grafana datasource | `clickhouse-1c/grafana/provisioning/datasources/clickhouse.yml` | Существует | OK | — |
| 55 | ClickHouse/1C | Ingest Rust | `adk-rust/crates/aw-1c-ingest/` | Существует | OK | — |
| 56 | ClickHouse/1C | landing каталоги | Не в репозитории (runtime-директории) | mkdir в handover step 6.3 | OK | Создаются при bootstrap |
| 57 | **Gateway** | Proxmox web gateway | `ansible/deploy_proxmox_web_gateway.yml` | Существует | OK | — |
| 58 | **Rust crates** | Все целевые крейты | 56 членов workspace в `adk-rust/Cargo.toml` | Все `Cargo.toml` найдены | OK | — |
| 59 | Rust crates | Quality gate | `adk-rust/crates/quality-gate/` + `scripts/quality-gate.sh` | Существует | OK | — |
| 60 | **Ansible** | `deploy_aw_server.yml` | `ansible/deploy_aw_server.yml` | Существует | OK | — |
| 61 | Ansible | `deploy_aw_windows.yml` | `ansible/deploy_aw_windows.yml` | Существует | OK | — |
| 62 | Ansible | `deploy_detmir_portal.yml` | `ansible/deploy_detmir_portal.yml` | Существует | OK | — |
| 63 | Ansible | `deploy_proxmox_web_gateway.yml` | `ansible/deploy_proxmox_web_gateway.yml` | Существует | OK | — |
| 64 | Ansible | `deploy_grafana_check.yml` | `ansible/deploy_grafana_check.yml` | Существует | OK | — |
| 65 | Ansible | `inventory.ini` | `ansible/inventory.ini` | Существует | **⚠️ CREDENTIALS LEAK** | Plaintext credentials detected; values redacted |
| 66 | Ansible | `inventory.example.ini` | `ansible/inventory.example.ini` | Существует | OK | — |
| 67 | **Docs/runbooks** | `adk-rust/RUNBOOK.md` | Существует | OK | OK | — |
| 68 | Docs/runbooks | `docs/preparation.md` | Существует | OK | OK | — |
| 69 | Docs/runbooks | `docs/deployment.md` | Существует | OK | OK | — |
| 70 | Docs/runbooks | `docs/runbook.md` | Существует | OK | OK | — |
| 71 | Docs/runbooks | `docs/operations.md` | Существует | OK | OK | — |
| 72 | Docs/runbooks | `docs/windows/deployment.md` | Существует | OK | OK | — |
| 73 | Docs/runbooks | `docs/OPERATIONS_RUNBOOK_WORKTIME_RU.md` | Существует | OK | OK | — |
| 74 | Docs/runbooks | `docs/GRAFANA_DASHBOARDS_RU.md` | Существует | OK | OK | — |
| 75 | Docs/runbooks | `docs/PORTAL_RU.md` | Существует | OK | OK | — |
| 76 | Docs/runbooks | `clickhouse-1c/README.md` | Существует | OK | OK | — |
| 77 | **Root scripts** | `check-aw-data.sh` (Rust wrapper) | Существует (shell → Rust fallback) | OK | OK | — |
| 78 | Root scripts | `check-aw-full.sh` (Rust wrapper) | Существует (shell → Rust fallback) | OK | OK | — |
| 79 | Root scripts | `scripts/prod_rollout.sh` | Существует | OK | OK | — |

---

## Найденные противоречия

### 1. AGENTS.md vs handover план: `clickhouse-1c/ai/`
Первичный вывод был ошибочным. Директория `clickhouse-1c/ai/` существует и входит в разрешенный Python island. Противоречия нет.

### 2. Handover план vs код: `ru-patch-v5.js`
Handover (строка 185) проверяет URL `/js/ru-patch-v5.js` — это корректно после
деплоя через Ansible. Но handover упоминает файл так, будто он лежит в
`aw-server/`, тогда как в репозитории исходник называется `aw-ru-patch.js`,
а в `ru-patch-v5.js` переименовывается при деплое (Ansible task строка 450).

### 3. Handover план vs код: `grafana/` структура
Handover (секция 7) указывает `grafana/` для Grafana/Influx. В реальности:
- Основные дашборды лежат плоскими JSON в корне `grafana/` (нет provisioning-субдиректории)
- Отдельный стек `grafana-1c/` с собственным `docker-compose.yml` для MSSQL/Postgres 1C
- ClickHouse-1C имеет свой provisioning в `clickhouse-1c/grafana/provisioning/`

Handover-план не отражает это разделение.

### 4. Ansible: лишние playbook
В handover перечислены 5 playbook, но в `ansible/` существуют 17 файлов, включая:
- `deploy_grafana_dashboards.yml` (не упомянут)
- `deploy_file_1c_windows_telemetry.yml` (не упомянут)
- `deploy_file_1c_analytics.yml` (не упомянут)
- `deploy_dlp_evidence_sync.yml` (не упомянут)
- `deploy_dlp_full_stack.yml` (не упомянут)
- `deploy_aw_pfsense_poller.yml` (не упомянут)
- `audit_cryptopro_windows.yml` (не упомянут)
- `post_validate_aw_windows.yml` (не упомянут)
- `provision_proxmox_ct_and_deploy_aw.yml` (не упомянут)
- `provision_proxmox_ct_matrix_and_deploy_aw.yml` (не упомянут)
- `deploy_tsj_guardian_bot_proxmox.yml` (не упомянут)
- `install_full_stack.yml` (не упомянут)

Это не ошибка, но handover не полон.

### 5. DLP service файлы — в поддиректориях
Handover проверяет `systemctl status aw-dlp-policy-engine` и
`aw-dlp-case-management`, что корректно. Но `.service` файлы лежат не
напрямую в `aw-server/`, а в поддиректориях:
`aw-server/dlp-policy-engine/`, `aw-server/dlp-case-management/`,
`aw-server/dlp-compliance/`. Это не влияет на runtime.

---

## MISSING (некритично)

1. **`secrets/`** — не существует, вместо него `private-config/` с `.gitignore`
   и `deploy.env.example`.

---

## Blocker (требует немедленного внимания)

### 🔴 CRITICAL: Production credentials в открытом виде
Файл **`ansible/inventory.ini`** игнорируется Git, но локально содержит production credentials в plaintext. Значения намеренно не приводятся в этом документе: audit-файлы нельзя превращать в копию секретов. Проверять только факт наличия `ansible_password`/host credentials и немедленно переносить их в vault или внешние переменные.

Это **реальные production credentials**:
- Production credentials из ignored inventory: Proxmox user password, AW server user password, Windows RDP administrator password. Значения не фиксировать в git, docs, logs или reports.

Это прямое нарушение AGENTS.md п. 4: "Never add real secrets from `secrets/`,
private `.env`, or host credentials."

**Рекомендация:** Немедленно заменить на переменные окружения или vault,
затем ротировать скомпрометированные пароли.

---

## Requires human approval

Следующие действия из handover-плана НЕЛЬЗЯ выполнять без подтверждения:

| Команда | Почему опасно |
|---------|---------------|
| `ansible-playbook -i inventory.ini deploy_aw_server.yml` | Реальный production деплой с живыми credentials |
| `ansible-playbook -i inventory.ini deploy_aw_windows.yml` | Может пересоздать Windows tasks, прервать сбор данных |
| `ansible-playbook -i inventory.ini deploy_detmir_portal.yml` | Перезапустит portal на production |
| `ansible-playbook -i inventory.ini deploy_grafana_check.yml` | Может изменить Grafana datasource |
| `systemctl restart aw-worktime-api` | Прервёт active reports |
| `systemctl start aw-worktime-influx-exporter.service` | Может записать дубли/битые данные в InfluxDB |
| `systemctl start aw-dlp-influx-exporter.service` | Аналогично |
| `cargo build --release --workspace` | Долгая компиляция (56 крейтов), может занять 20+ мин |
| `docker compose up -d` в `clickhouse-1c/` | Поднимет ClickHouse, может конфликтовать с существующим |

Все команды с прямым обращением к production (через inventory.ini или SSH)
требуют явного разрешения.

---

## Next actions

1. **Срочно:** убрать plaintext credentials из `ansible/inventory.ini`: перенести доступы в Ansible Vault или внешние переменные и ротировать уже раскрытые пароли.
2. **Сделано в текущем цикле:** AGENTS.md и handover-план дополнены правилом
   sanitized audit, недостающими Ansible playbook и структурой `grafana/` vs
   `grafana-1c/` vs `clickhouse-1c/grafana/`.
3. **Добавить `grafana/provisioning/`** структуру в корень для единого
   подхода к дашбордам (сейчас JSON плоские — работает только через Ansible
   копирование).
4. **Проверить ClickHouse .env** — если содержит реальные credentials,
   добавить в .gitignore и вычистить из истории.
