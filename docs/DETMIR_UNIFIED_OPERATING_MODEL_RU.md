# DetMir: Единая рабочая модель системы

Дата фиксации: `2026-05-24`

Последнее runtime-уточнение: `2026-05-30`

Этот файл предназначен как единая рабочая опора по `DetMir`: что именно входит в систему, где это живет, каким инструментарием проект надо планировать и сопровождать, и какой операционный контур считать промышленным.

Если старые документы расходятся с этим файлом по адресам или runtime-ролям, для текущей эксплуатации приоритет у этого файла.

Связанная security-основа: `docs/DETMIR_THREAT_MODEL_RU.md` фиксирует текущую
операционную модель угроз. Это рабочая модель для платформы операционного
контроля и технического аудита, а не формальная сертификационная модель ФСТЭК.

Связанное продуктовое позиционирование:
`docs/DETMIR_RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md` фиксирует безопасный
заход для реестра российского ПО: DetMir как платформа операционного контроля и
управления ИТ-инфраструктурой, с ориентиром на класс `09.10`, без заявления
сертифицированной DLP/SIEM/EDR/XDR/СЗИ.

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

### 2.1 Runtime snapshot после полной проверки 2026-05-28

Проверка выполнялась как production-contour test, а не только как HTTP ping.
Покрыты:

- `AW-rus` API/WebUI на `10.10.10.13:5600`;
- worktime/management API на `10.10.10.13:5610`;
- Windows/RDP host `192.168.100.18` через WinRM/SSH/Scheduled Tasks;
- `1C/file analytics` backend на `10.10.10.2:8710`;
- Proxmox/nginx gateway на `10.10.10.2`;
- Grafana на `10.10.10.11:3000`;
- browser smoke через Playwright по operator-facing страницам.

Фактический результат после стабилизации:

| Проверка | Результат |
|---|---|
| `./check-aw-full.sh` | `FRESH=8 STALE=0 DEAD=0` |
| `aw-rus-healthd.py --json` | `ok=13 warn=0 fail=0` |
| `dlp-health-check --json` | `ok=20 warn=0 fail=0` |
| `systemctl --failed` на `10.10.10.13` | `0 loaded units listed` |
| Playwright browser smoke | `14/14` страниц открылись |
| Grafana authenticated API/UI smoke | login OK, `19` dashboards в `/api/search`, все ключевые `1C File`/`DetMir` dashboards открылись |
| Grafana datasource health | `OK` для `clickhouse-1c`, `InfluxDB-AW`, `loki`, Proxmox/pfSense Influx datasources |
| Python unit tests по AW server/worktime/DLP/exporters | `36 passed` |
| `proxmox.test_tsj_guardian_bot` | `25 tests OK` |
| Windows `ActivityWatch Recovery` | `Last Result: 0` |

Ключевые runtime-факты на момент фиксации:

- `activitywatch-server`, `aw-worktime-api`, `aw-worktime-ui-bridge.timer`, `aw-worktime-autoheal.timer` активны;
- свежие buckets: `aw-watcher-afk_*`, `aw-watcher-window_*`, `aw-worktime-sessions_*`, `aw-dlp-endpoint-signals_*`;
- `aw-dlp-incidents_*`, `aw-dlp-review_*`, `aw-dlp-rules_*` могут быть event-driven и не обязаны двигаться каждую минуту;
- Grafana dashboards без авторизации корректно редиректят на login, это не считается отказом; с сохраненной admin-учеткой проверены фактические страницы и datasource health;
- gateway `/go/file1c-brief`, `/go/file1c-actions`, `/go/aw-ui` ведет на рабочие внутренние surface.

### 2.3 Runtime-семантика операторских сигналов от 2026-05-30

После Phase 8 операторские проверки должны читаться по смыслу статуса, а не только по возрасту последнего события.

| Статус | Как трактовать |
|---|---|
| `FRESH` | Данные свежие, источник сейчас активен или недавно обновлялся. |
| `INACTIVE` | Нормальное состояние для desktop/window/DLP endpoint buckets, если worktime-сессия свежая, но интерактивной активности нет. Это не инцидент. |
| `EVENT-DRIVEN` | Нормальное состояние для buckets, которые пишутся только при событии: `aw-session-events_*`, `aw-dlp-incidents_*`, `aw-dlp-review_*`, `aw-dlp-rules_*`. Отсутствие новых событий само по себе не отказ. |
| `STALE` | Потенциальная деградация активного источника; проверять collector/session placement. |
| `DEAD` / `EMPTY` | Отказ или неинициализированный обязательный источник; требуется диагностика. |

Операторские правила:

- `./check-aw-full.sh` с `FRESH=8 STALE=0 DEAD=0` считается зеленым контуром, даже если отдельные строки показывают `INACTIVE` или `EVENT-DRIVEN`.
- `./check-aw-data.sh` должен использовать ту же семантику, что и full-check; `EVENT-DRIVEN` и `INACTIVE` не являются поводом для ручного recovery.
- SLO-строка бота `aw_rus_slo: recovered ... current_sample=OK ... budget_remaining_seconds<0` означает исторически сожженный error budget при здоровом текущем контуре. Это не активная авария.
- SLO становится инцидентом только при текущем `current_sample=FAIL` и исчерпанном бюджете или при stale SLO summary.
- `aw-browser-smoke.timer` хранит ограниченное число запусков через `AW_BROWSER_SMOKE_KEEP_RUNS` и ограничен `TimeoutStartSec=180`; рост `/var/lib/activitywatch/browser-smoke` выше нескольких сотен MB надо считать regression в retention/config.

### 2.2 Стабилизация management report, bridge и recovery от 2026-05-28

До стабилизации слабые места были такими:

- холодный `management report` на `:5610` занимал примерно `38-58s`;
- `aw-worktime-autoheal` мог считать тяжелый management warm частью health-check и перезапускать `aw-worktime-api`;
- `aw-worktime-ui-bridge.service` периодически ловил `start-limit-hit`, хотя затем восстанавливался;
- Windows task `ActivityWatch Recovery` оставался с `Last Result: 1`, несмотря на зеленый основной сбор данных.

Что изменено:

| Компонент | Файл | Решение |
|---|---|---|
| Management report API | `aw-server/aw-worktime-api.py` | Добавлены in-process events cache, build lock на `(host, report_date)`, переиспользование уже построенного payload для trend и чтение historical cache без TTL. |
| Autoheal | `aw-server/aw-worktime-autoheal.sh` | Management warm больше не является обязательным health probe; timeout warm увеличен до `60s`. |
| Worktime UI bridge | `aw-server/aw-worktime-ui-bridge.service` | `StartLimitBurst` поднят до `20`, чтобы штатные timer-запуски не переводили unit в `start-limit-hit`. |
| Windows recovery | `windows/ActivityWatch.Windows.Common.psm1` | Усилен hidden wrapper, добавлен fallback через `schtasks.exe`, recovery task выбирает live interactive user, если SYSTEM path на хосте проблемный. |

Измеренный эффект:

| Сценарий | До | После |
|---|---:|---:|
| Cold/cold-ish management JSON | `38-58s` | около `11s` на сервере |
| Повторный management JSON из cache | нестабильно | около `0.006s` на сервере |
| Внешний первый request | до `58s` | около `18.9s` |
| Внешний повторный request | нестабильно | около `0.215s` |

Операционное ограничение:

- полный `hardening-recovery.ps1` на Windows host может упираться в CIM/ScheduledTasks `message filter`;
- для текущего production recovery закреплен рабочий путь через `schtasks.exe` и live interactive admin principal;
- не запускать полный hardening-прогон без причины, если buckets свежие и `ActivityWatch Recovery` уже `Last Result: 0`.

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
| `docs/DETMIR_THREAT_MODEL_RU.md` | рабочая модель угроз и границы security-позиционирования |
| `docs/DETMIR_RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md` | стратегия позиционирования для реестра российского ПО |
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
