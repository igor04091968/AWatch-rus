# DetMir: Единая рабочая модель системы

Дата фиксации: `2026-05-24`

Этот файл предназначен как единая рабочая опора по `DetMir`: что именно входит в систему, где это живет, каким инструментарием проект надо планировать и сопровождать, и какой операционный контур считать промышленным.

Если старые документы расходятся с этим файлом по адресам или runtime-ролям, для текущей эксплуатации приоритет у этого файла.

## 1. Назначение

`DetMir` в этом репозитории это не только `ActivityWatch Server`.
Это полный production-контур:

- сбор активности и DLP-сигналов с Windows/RDP;
- сервер `AW-rus` с RU WebUI, API, health и management-отчетами;
- операторский узел `Proxmox/DetMirAuto`;
- сетевой контур `pfSense + OpenVPN`;
- Telegram bot для инцидентов, auto-heal, recovery и операционных команд;
- `Hayabusa` как DFIR enrichment слой;
- визуализация и аналитика в `Grafana`;
- отдельный `1C/file analytics` контур через `ClickHouse/Grafana/API`.

Цель проекта:

- держать рабочий production-контур без дрейфа между кодом, runtime и документацией;
- обеспечивать управляемый deploy, health-check, auto-heal и incident response;
- поддерживать дальнейшее развитие без разрушения текущей рабочей системы.

## 2. Текущий подтвержденный runtime

Ниже не историческая схема, а рабочая опорная карта.

| Узел | Роль |
|---|---|
| `10.10.10.1` | `pfSense`, firewall, VPN, ACL, OpenVPN export target |
| `10.10.10.2` | `Proxmox/DetMirAuto`, web gateway, Telegram bot, operator entrypoint |
| `10.10.10.13` | основной `AW-rus` server, health, worktime/reporting, `Hayabusa` server-side processing |
| `192.168.100.18` | `SHARKON2025`, Windows/RDP host, collectors, worktime session path, EVTX export |

Практический вывод:

- серверный путь `AW-rus` сейчас должен считаться `10.10.10.13`;
- операторский и gateway-контур должен считаться `10.10.10.2`;
- Windows production-host для `DetMir` сейчас `192.168.100.18`, а не старые упоминания `192.168.100.21`.

## 3. Полный функциональный состав DetMir

### 3.1 Ядро AW-rus

| Функция | Где реализована |
|---|---|
| ActivityWatch API и WebUI | `aw-server/`, `activitywatch-server.service` |
| RU WebUI patch | `aw-server/`, `docs/FULL_DEPLOYMENT_MANUAL_RU.md` |
| DLP overlay в WebUI | `aw-server/`, buckets `aw-dlp-review_*`, `aw-dlp-rules_*` |
| Health daemon | `aw-server/aw-rus-healthd.py` |
| Расширенный health-check | `aw-server/health-check.sh`, `check-aw-full.sh`, `check-aw-data.sh` |
| Management/worktime reports | server-side API на `:5610`, `docs/runbook.md`, `docs/worktime_aql_detmir.md` |

### 3.2 Windows/RDP контур

| Функция | Где реализована |
|---|---|
| Массовый deploy | `windows/deploy-domain-users.ps1`, `windows/deploy-ensemble.ps1` |
| Single-user deploy | `windows/deploy-single-user.ps1` |
| Validation | `windows/validate-deployment.ps1` |
| Recovery/hardening | `windows/hardening-recovery.ps1` |
| AFK/window watchers | `ActivityWatch` watchers + scheduled tasks |
| Browser domains | `windows/browser-domains-native-collector.ps1` |
| DLP endpoint signals | `windows/dlp-endpoint-signals-collector.ps1` |
| Email/DLP path | `windows/email-outbound-collector.ps1` |
| Worktime session presence | `windows/worktime-session-collector.ps1` |
| Incident artifacts / screenshots / EVTX export | `windows/export-evtx-for-hayabusa.ps1` и related runtime scripts |

### 3.3 DLP и расследование

| Функция | Где реализована |
|---|---|
| Endpoint DLP сигналы | bucket `aw-dlp-endpoint-signals_*` |
| DLP incidents | bucket `aw-dlp-incidents_*` |
| Review/rules operator flow | buckets `aw-dlp-review_*`, `aw-dlp-rules_*`, WebUI overlay |
| DLP report/scheduler | `aw-server` runtime + tests/docs |
| Transport/self-test telemetry | `aw-health-check` и Windows transport telemetry |
| Incident follow-up | operator path + `Hayabusa` enrichment |

### 3.4 Worktime и managerial layer

| Функция | Где реализована |
|---|---|
| Presence по RDP-сессиям | bucket `aw-worktime-sessions_*` |
| Работа по окнам/AFK | buckets `aw-watcher-window_*`, `aw-watcher-afk_*` |
| Web category worktime | bucket `aw-detmir-web-category_*` |
| AQL-шаблоны | `docs/worktime_aql_detmir.md` |
| Management report API | `:5610/reports/worktime/management` |
| Owner/department reporting | aliases и manager-facing filters в server-side report layer |

### 3.5 Proxmox / operator / bot

| Функция | Где реализована |
|---|---|
| LXC/bootstrap/deploy | `proxmox/create-ct.sh`, `proxmox/push-aw-artifacts.sh`, `ansible/` |
| Telegram incident bot | `proxmox/tsj_guardian_bot.py` |
| Auto-heal и recovery path | `tsj_guardian_bot.py`, `.planning/phases/02-operator-bot-recovery/` |
| OpenVPN config generation/export | `proxmox/pfsense_openvpn_client_export.php`, bot runtime |
| Proxmox restore/snapshot operator flow | bot pending restore flow + playbooks/runtime |
| Web gateway / internal entrypoint | `docs/runbook.md`, nginx/gateway rollout |

### 3.6 pfSense / network / VPN

| Функция | Где реализована |
|---|---|
| Firewall/ACL | `pfSense` ruleset |
| OpenVPN user access | `pfSense` + bot export path |
| pfSense telemetry в AW | `pfsense/pfsense-aw-poller.py`, `docs/pfsense.md` |
| Buckets `aw-pfsense-*` | health/interfaces/gateways |
| Routing between server and Windows | `pfSense` rules, не Windows-local hacks |

### 3.7 Форензика Windows логов (Hayabusa)

| Функция | Где реализована |
|---|---|
| EVTX export на Windows | `windows` runtime/export scripts |
| Intake на сервере | `10.10.10.13`, drop/inbox flow |
| Processing/reporting | `aw-hayabusa`, `/opt/hayabusa`, server-side services |
| Bounded integration с AW-rus | `docs/hayabusa-aw-rus-integration-2026-05-14.md` |
| Operator guidance | `docs/hayabusa-operator-ib-guide-2026-05-14.md`, `docs/runbook.md` |

Ключевая граница:

- `Hayabusa` это enrichment для incident/forensics;
- это не замена обычного realtime health/collector/DLP runtime.

### 3.8 Grafana / monitoring / 1C analytics

| Контур | Когда использовать |
|---|---|
| `grafana/` dashboards | AW-rus runtime, RDP activity, DLP, management/security overview |
| `grafana-1c/` | когда есть удобный SQL/read-only KPI путь из 1С |
| `clickhouse-1c/` | когда 1С файловая и нужен audit/timeline/detections/cases слой |

По `clickhouse-1c/`:

- это отдельный industrial scaffold для `file-1C analytics`;
- он не заменяет AW-rus, а добавляет audit/timeline/cases/company-intelligence контур;
- строится вокруг `landing -> ETL -> ClickHouse -> Grafana + AI Investigator`.

## 4. Главные пользовательские и операторские входы

Внутренний операторский доступ должен идти через VPN/внутреннюю сеть, не через случайно открытые наружу порты.

Базовые входы:

- `https://10.10.10.2/` — gateway;
- `https://10.10.10.2/go/proxmox-gui` — Proxmox GUI;
- `https://10.10.10.2/go/file1c-brief` — management/file-1C brief;
- `https://10.10.10.2/go/file1c-actions` — actions;
- `http://10.10.10.13:5600/api/0/info` — AW-rus API health;
- `http://10.10.10.13:5610/reports/worktime/management` — management reporting API.

Telegram bot `DetMirAuto` обязан покрывать:

- incident detection;
- `/ack`, `/heal`, `/run check`, `/run support`, `/run fallback`, `/status`;
- `/aw_dlp_check`, `/dlp_mode`, `/dlp_mode_toggle` для операторского DLP-контура;
- OpenVPN config issuance/export;
- operator-safe workflows по recovery и restore;
- отсутствие ложных алертов при transient self-heal.

Операторская семантика меню бота:

- DLP-кнопка обязана показывать текущий режим и следующее действие:
- `DLP сейчас: наблюдение | включить блокировку`
- `DLP сейчас: блокировка | включить наблюдение`
- `DLP сейчас: смешанный | выровнять в блокировку`
- Кнопка `Форензика Windows логов` — человеко-понятный вход в bounded Hayabusa path для Windows EVTX / DFIR follow-up.
- Если Telegram-клиент держит устаревшую custom-keyboard, оператор должен нажать `Статус` или `/start`: бот обязан переслать свежую клавиатуру с актуальным DLP label.

## 5. Промышленный набор инструментов для планирования и сопровождения

Ниже минимальный правильный стек, который стоит считать базовым для `DetMir`.

### 5.1 Обязательное ядро GSD

Эти компоненты нужны постоянно:

- `gsd-phase`
- `gsd-plan-phase`
- `gsd-planner`
- `gsd-phase-researcher`
- `gsd-plan-checker`
- `gsd-execute-phase`
- `gsd-executor`
- `gsd-verifier`
- `gsd-verify-work`
- `gsd-review`
- `gsd-code-reviewer`
- `gsd-code-fixer`
- `gsd-debug`
- `gsd-debugger`
- `gsd-secure-phase`
- `gsd-security-auditor`

### 5.2 Нужные supporting-компоненты

- `gsd-doc-writer`
- `gsd-doc-synthesizer`
- `gsd-doc-verifier`
- `gsd-integration-checker`
- `gsd-intel-updater`
- `gsd-codebase-mapper`

Для UI-фаз подключать только по необходимости:

- `gsd-ui-phase`
- `gsd-ui-researcher`
- `gsd-ui-checker`
- `gsd-ui-auditor`

### 5.3 Операционные skills для живого DetMir

- `aw-ops-checks`
- `aw-russian-collectors-guard`
- `bot`
- `autonomous-skill` для длинных recovery/ops-циклов

### 5.4 Что не нужно включать в базовый industrial-контур

По умолчанию не поднимать как обязательную часть процесса:

- `gsd-fast`
- `gsd-quick`
- `gsd-sketch`
- большинство `gsd-ns-*`
- `gsd-workstreams`

Причина простая:

- `DetMir` уже не greenfield и не playground;
- лишняя оркестрация здесь опаснее, чем полезна;
- нужен жесткий управляемый контур, а не разросшийся набор экспериментальных режимов.

## 6. Что именно надо реанимировать из .codex

Практическая схема такая:

- использовать agent-описания из `/home/igor/.codex/agents/gsd-*` как действующее ядро исполнителей;
- вернуть orchestrator-skills из `/home/igor/.codex/skills_disabled/2026-05-21-current-prune/` для верхнеуровневого workflow;
- hooks `gsd-phase-boundary.sh`, `gsd-statusline.js`, `gsd-workflow-guard.js` держать как сервисную обвязку, а не как замену основному процессу.

Минимум к восстановлению как orchestrator layer:

- `gsd-phase`
- `gsd-plan-phase`
- `gsd-execute-phase`
- `gsd-review`
- `gsd-verify-work`
- `gsd-debug`
- `gsd-secure-phase`
- `gsd-ui-phase`

## 7. Рабочий lifecycle для DetMir

### 7.1 Планирование

Каждая нетривиальная работа должна идти через фазу, а не через бессистемные правки.

Обязательная цепочка:

1. зафиксировать изменение в `.planning/ROADMAP.md` и состоянии проекта;
2. открыть фазу в `.planning/phases/<NN>-<slug>/`;
3. собрать `PLAN.md` через `gsd-plan-phase`;
4. выполнить research/check loop до defensible плана;
5. только после этого идти в выполнение.

### 7.2 Исполнение

Исполнение должно идти волнами, а не одним большим неоткатываемым махом.

Обязательные правила:

- backup-first перед risky runtime changes;
- server/network/windows changes не смешивать без явной причины;
- после каждого meaningful шага оставлять проверяемый след в артефактах и сервисных логах;
- не считать задачу завершенной без реальной проверки на живом контуре.

### 7.3 Проверка и приемка

После выполнения обязательно:

- `gsd-verifier` на достижение целевого эффекта;
- `gsd-review` на код/риск/регрессии;
- `gsd-verify-work` на пользовательскую приемку;
- `gsd-secure-phase`, если фаза трогала сеть, доступы, VPN, secrets, bot, `pfSense`, `Windows` admin path.

### 7.4 Документация

После фазы обновлять не “когда-нибудь”, а сразу:

- runbook;
- relevant docs;
- phase `SUMMARY.md`;
- если нужно, `STATE.md` и `ROADMAP.md`.

Для `DetMir` это обязательно, потому что основная историческая боль проекта была не в отсутствии кода, а в расхождении между repo, runtime и реальностью.

## 8. Базовый комплект артефактов

Для каждой фазы и для всей системы нужны конкретные файлы, а не размытые договоренности.

| Артефакт | Назначение |
|---|---|
| `.planning/ROADMAP.md` | очередь и границы проекта |
| `.planning/STATE.md` | текущее состояние production-контура |
| `.planning/phases/<NN>-<slug>/PLAN.md` | исполнимый план |
| `.planning/phases/<NN>-<slug>/SUMMARY.md` | факт выполненного |
| `VERIFICATION.md` | доказательство результата |
| `REVIEW.md` | findings по code/review |
| `SECURITY.md` | security findings по risky фазам |
| `UAT.md` | операторская приемка |
| `docs/runbook.md` | живая эксплуатация |
| этот файл | единая карта системы и рабочего процесса |

## 9. Операционный минимум, который нельзя терять

Система считается реально сопровождаемой только если одновременно живы все эти контуры:

- `AW-rus API` отвечает;
- buckets по `afk/window/worktime/DLP` свежие;
- `aw-rus-healthd` и `aw-health-check` не врут и не шумят ложными fail;
- Telegram bot не генерирует ложные auto-heal incidents;
- `pfSense` ACL соответствуют фактическому runtime;
- есть рабочий Windows deploy/validate/recovery path;
- есть рабочий `Hayabusa` follow-up path;
- Grafana/importable dashboards version-controlled;
- management/reporting path на `:5610` не сломан;
- docs и phase-artifacts отражают реальное состояние, а не прошлую эпоху.

## 10. Что считать правильным направлением развития

Приоритет для `DetMir` сейчас такой:

1. не расширять систему ценой потери baseline stability;
2. сначала держать зеленым production runtime;
3. потом усиливать regression guards и operator path;
4. затем развивать `DLP`, `management`, `Hayabusa`, `file-1C analytics`;
5. любой новый слой вводить только так, чтобы он не ломал существующий recovery path.

## 11. Краткое правило принятия решений

Если есть выбор между:

- “быстро добавить еще один контур”;
- и “сделать существующий контур проверяемым, документированным и устойчивым”,

для `DetMir` правильный выбор почти всегда второй.

Это и есть промышленный режим сопровождения этой системы.
