# DetMir: Отчет по изменениям за последние 24 часа

Дата фиксации: `2026-05-26 07:55 MSK`

## Источники

- `tmux` session `codex`, scrollback выгружен в `/tmp/codex-tmux-last24.txt`
- текущий worktree `git status` в `<PROJECT_ROOT>`
- live-проверки на:
  - `<GATEWAY_HOST>` Proxmox
  - `<GRAFANA_HOST>` Grafana
  - `<AUX_SERVICE_HOST>` Loki/Alloy
  - `<AW_SERVER_HOST>` aw-server
  - `<FIREWALL_HOST>` pfSense
  - `<WINDOWS_HOST>` Windows RDP host

## Важная оговорка

`git log --since='24 hours ago'` пустой: за последние сутки изменения не были оформлены git-коммитами.

Рабочее дерево уже грязное и содержит больше изменений, чем было сделано в этом 24-часовом окне. Поэтому ниже зафиксированы только те изменения, которые явно подтверждаются `tmux`-историей и live-проверками.

## Что было сделано

### 1. Восстановление ActivityWatch-Russian

Симптом:
- на `<AW_SERVER_HOST>:5600` UI не обновлялся для `SHARKON2025`;
- `aw-server` был жив, но stale были `aw-watcher-afk`, `aw-watcher-window`, `aw-worktime-sessions`.

Действия:
- проверен `aw-server` и свежесть bucket’ов;
- через WinRM на `<WINDOWS_HOST>` проверены процессы, tasks и recovery-скрипты;
- запущены:
  - `ActivityWatch Recovery`
  - `ActivityWatch Launch [SHARKON2025_user5]`
- отдельно перезапущен зависший `worktime-session-collector`.

Итог:
- `aw-watcher-afk`, `aw-watcher-window`, `aw-session-events`, `aw-dlp-endpoint-signals`, `aw-worktime-sessions` снова стали fresh;
- итоговая серверная сводка: `FRESH=8 STALE=0 DEAD=0`.

### 2. Дополнена авто-сигнализация и самолечение `tsj-guardian-bot`

Проблема:
- фоновый цикл бота не видел stale AW collector bucket’ы;
- manual AW-check был отдельным путем и не участвовал в обычном incident/heal flow.

Сделано:
- в `tsj_guardian_bot.py` добавлен разбор `[FAIL] aw-rus:*`;
- AW collector freshness включен в стандартный background check cycle;
- добавлен Windows recovery path для:
  - `watcher-*`
  - `worktime-session-collector`
- добавен server-side rebuild worktime views после Windows remediation;
- тесты обновлены и прогнаны.

Затронутые файлы:
- [proxmox/tsj_guardian_bot.py](<PROJECT_ROOT>/proxmox/tsj_guardian_bot.py)
- [proxmox/test_tsj_guardian_bot.py](<PROJECT_ROOT>/proxmox/test_tsj_guardian_bot.py)
- [ansible/deploy_tsj_guardian_bot_proxmox.yml](<PROJECT_ROOT>/ansible/deploy_tsj_guardian_bot_proxmox.yml)
- [ansible/group_vars/proxmox-bot.example.yml](<PROJECT_ROOT>/ansible/group_vars/proxmox-bot.example.yml)

Проверка:
- `python3 -m py_compile proxmox/tsj_guardian_bot.py proxmox/test_tsj_guardian_bot.py`
- `python3 -m unittest proxmox.test_tsj_guardian_bot`
- `tsj-guardian-bot.service` перезапускался и работал в `active`.

### 3. Бот переведен на `igor` вместо `codex`

Проблема:
- production bot был завязан на `AI_EXEC_USER=codex`, `TMUX_USER=codex` и read-only пути под `/home/codex/infra-admin`;
- это ломало встроенный `codex-cli` support path после перехода на `igor`.

Сделано:
- переключены:
  - `AI_EXEC_USER=igor`
  - `TMUX_USER=igor`
  - `AI_CHAT_WORKDIR=<OPERATOR_HOME>`
- добавлены настраиваемые:
  - `PFSENSE_ENV_PATH`
  - `PFSENSE_INVENTORY_PATH`
- создан `igor`-readable bundle:
  - `<OPERATOR_HOME>/.config/tsj-bot/pfsense.env.readonly`
  - `<OPERATOR_HOME>/.config/tsj-bot/inventory.md`

Итог:
- диалоговая техподдержка и AI-эскалация в боте работают от `igor`;
- bot-side pfSense/inventory контекст больше не зависит от старого `codex`-домика.

### 4. Исправлен сетевой доступ `<GATEWAY_HOST> -> <WINDOWS_HOST>`

Root cause:
- на `pfSense <FIREWALL_HOST>`, интерфейс `opt1/MGMT`:
  - allow rule для `<GATEWAY_HOST> -> <WINDOWS_HOST>:22` был ниже `block all`;
  - allow rule для `<GATEWAY_HOST> -> <FIREWALL_HOST>:2022` был ниже `block all`;
  - rule для `<GATEWAY_HOST> -> <WINDOWS_HOST>:5985` отсутствовал.

Сделано:
- подняты нужные allow rules выше `block all`;
- добавлено недостающее правило на `5985`.

Итог:
- с `<GATEWAY_HOST>` подтвержден доступ на:
  - `<FIREWALL_HOST>:2022`
  - `<WINDOWS_HOST>:22`
  - `<WINDOWS_HOST>:5985`
- SSH вход на `<WINDOWS_HOST>` под `Администратор` был подтвержден.

### 5. Убрано ложное сообщение “автолечение невозможно”

Причина:
- в production `.env` бота была повреждена строка `AW_RUS_WORKTIME_HEAL_CMD`;
- бот слишком жестко реагировал на единичный таймаут worktime CSV.

Сделано:
- восстановлен `AW_RUS_WORKTIME_HEAL_CMD`;
- добавлен retry для получения worktime CSV;
- обновлены дефолты под `igor`.

Итог:
- `pending_incident = null`;
- циклы снова идут как `Check OK`.

### 6. Наведен порядок в Grafana folder/dashboard catalog

Изменения:
- folder `AWatch-rus` переименован в `DLP`;
- `pfSense System Dashboard` перенесен в folder `pfSense`;
- `LXC Containers Monitoring` перенесен в folder `LXC`;
- `Proxmox Influx 2.0 Dashboard` перенесен в folder `LXC`.

### 7. Починен `pfSense Logs Dashboard All Logs`

Проблема была не в Grafana, а в цепочке `pfSense -> Alloy -> Loki`.

Сделано:
- на `<FIREWALL_HOST>` поднят `syslogd`;
- на `<AUX_SERVICE_HOST>` в Alloy для pfSense syslog включен правильный формат `rfc3164`;
- отключен stale docker self-scrape path, забивавший `loki.write` старыми batch’ами;
- перезапущен `alloy`.

Итог:
- появились живые `job="pfsense"` streams в Loki;
- дашборд `pfSense Logs Dashboard All Logs` перестал быть пустым.

### 8. Починен `1C File - Operations Health`

Проблема:
- не обновлялись данные в `1C File - Operations Health`.

Сделано на `<WINDOWS_HOST>`:
- exporter больше не падает на недоступных `ibases.v8i`;
- `export-upload-file-1c-telemetry.ps1` научен брать `remoteKeyPath` из `deployment-config.json`;
- рабочий ключ закреплен как `C:\Users\USER1\.ssh\awops_ed25519`;
- исправлены права к `file1c-telemetry-state.json`.

Сделано на `<GATEWAY_HOST>`:
- прогнан ingest;
- `proofcheck` выведен в green.

Затронутые файлы:
- [windows/export-upload-file-1c-telemetry.ps1](<PROJECT_ROOT>/windows/export-upload-file-1c-telemetry.ps1)
- [ansible/deploy_file_1c_windows_telemetry.yml](<PROJECT_ROOT>/ansible/deploy_file_1c_windows_telemetry.yml)

Итог:
- `1C File - Operations Health` снова обновляется;
- свежесть downstream была подтверждена через ingest и proofcheck.

### 9. Рабочие ключи выданы всем пользователям Windows-хоста

Сделано:
- ключ `awops_ed25519` разложен по локальным профилям:
  - `USER1`
  - `USER2`
  - `USER3`
  - `USER4`
  - `USER5`
  - `Администратор`
- `remoteKeyPath` перестал быть жестко привязан только к `USER1`;
- права на `file1c-telemetry-state.json` выданы рабочим пользователям.

Итог:
- схема больше не зависит от одного профиля;
- scheduled upload path продолжает работать.

### 10. Поднят management board по файловой 1С

Сделано:
- добавлен новый Grafana dashboard `1C File - Management Board`;
- обновлены manager brief links и gateway route.

Live-итог:
- `uid=1c-file-mgmt`
- `go/grafana-1c` редиректит на новый board

### 11. Поднят первый financial board по файловой 1С

Сделано:
- добавлен SQL слой:
  - [clickhouse-1c/clickhouse/init/05_financial_reporting.sql](<PROJECT_ROOT>/clickhouse-1c/clickhouse/init/05_financial_reporting.sql)
- добавлен dashboard:
  - [clickhouse-1c/grafana/provisioning/dashboards/files/1c-financial-reporting.json](<PROJECT_ROOT>/clickhouse-1c/grafana/provisioning/dashboards/files/1c-financial-reporting.json)
- обновлен gateway route:
  - `go/file1c-finance`

Итог:
- financial screen работает как честный transitional layer;
- live readiness была `proxy_only`;
- `postings_table_rows = 0`, то есть настоящие проводки еще не поступают.

### 12. Проведено расследование production-source для `postings`

Что подтверждено на `<WINDOWS_HOST>`:
- готового `toolkit`/REST/service под postings нет;
- порт `6003` и типовые service-порты не слушаются;
- текущий Windows upload path шлет только telemetry/snapshot, без `postings`.

Что проверено:
- наличие `1C 8.3.27` и `comcntr.dll`;
- COMConnector в user-token path;
- интерактивные scheduled tasks под `USER1`;
- явные 1С-креды:
  - `Администратор / <WINDOWS_PASSWORD>`
  - `user / <WINDOWS_PASSWORD>`
  - `user1 / <WINDOWS_PASSWORD>`

Итог:
- blocker остался прежним: нужен реальный пользователь 1С с read-only доступом;
- без него production extractor проводок не собрать.

### 13. Поднят отдельный `1C File - Telemetry Board`

Сделано:
- добавлен новый dashboard:
  - [clickhouse-1c/grafana/provisioning/dashboards/files/1c-telemetry-board.json](<PROJECT_ROOT>/clickhouse-1c/grafana/provisioning/dashboards/files/1c-telemetry-board.json)
- добавлена ссылка на него из `1C File - Management Board`;
- добавлен gateway route:
  - `go/file1c-telemetry`

Live-итог:
- в Grafana зарегистрирован `uid=1c-file-telemetry`;
- все 1C dashboards лежат в folder `1C File Analytics`;
- `https://<GATEWAY_HOST>/go/file1c-telemetry` редиректит на новый board.

### 14. Live-путь `1C File Analytics` в Grafana был приведен к устойчивому состоянию

Выяснилось:
- file-based provisioning на `<GRAFANA_HOST>` не был основной точкой для 1C dashboards;
- текущие `file-1c` dashboards жили в DB Grafana.

Сделано:
- на CT201 добавлен отдельный provisioning provider `1C File Analytics`;
- в него выложены:
  - `1c-management-board.json`
  - `1c-telemetry-board.json`
- `grafana-server` перезапущен;
- через SQLite БД Grafana подтверждено наличие:
  - `1c-file-mgmt`
  - `1c-file-finance`
  - `1c-file-ops`
  - `1c-file-telemetry`
  - folder `file-1c / 1C File Analytics`

## Файлы, которые точно редактировались в этом окне по данным tmux

- [proxmox/tsj_guardian_bot.py](<PROJECT_ROOT>/proxmox/tsj_guardian_bot.py)
- [proxmox/test_tsj_guardian_bot.py](<PROJECT_ROOT>/proxmox/test_tsj_guardian_bot.py)
- [ansible/deploy_tsj_guardian_bot_proxmox.yml](<PROJECT_ROOT>/ansible/deploy_tsj_guardian_bot_proxmox.yml)
- [ansible/group_vars/proxmox-bot.example.yml](<PROJECT_ROOT>/ansible/group_vars/proxmox-bot.example.yml)
- [windows/export-upload-file-1c-telemetry.ps1](<PROJECT_ROOT>/windows/export-upload-file-1c-telemetry.ps1)
- [ansible/deploy_file_1c_windows_telemetry.yml](<PROJECT_ROOT>/ansible/deploy_file_1c_windows_telemetry.yml)
- [clickhouse-1c/clickhouse/init/05_financial_reporting.sql](<PROJECT_ROOT>/clickhouse-1c/clickhouse/init/05_financial_reporting.sql)
- [ansible/deploy_proxmox_web_gateway.yml](<PROJECT_ROOT>/ansible/deploy_proxmox_web_gateway.yml)

Новые dashboard JSON, подтвержденные live-выкладкой:
- [clickhouse-1c/grafana/provisioning/dashboards/files/1c-financial-reporting.json](<PROJECT_ROOT>/clickhouse-1c/grafana/provisioning/dashboards/files/1c-financial-reporting.json)
- [clickhouse-1c/grafana/provisioning/dashboards/files/1c-management-board.json](<PROJECT_ROOT>/clickhouse-1c/grafana/provisioning/dashboards/files/1c-management-board.json)
- [clickhouse-1c/grafana/provisioning/dashboards/files/1c-telemetry-board.json](<PROJECT_ROOT>/clickhouse-1c/grafana/provisioning/dashboards/files/1c-telemetry-board.json)

## Проверки, которые были явно пройдены

- `python3 -m py_compile proxmox/tsj_guardian_bot.py proxmox/test_tsj_guardian_bot.py`
- `python3 -m unittest proxmox.test_tsj_guardian_bot`
- `./check-aw-full.sh` дал `FRESH=8 STALE=0 DEAD=0`
- gateway deploy на Proxmox прошел успешно
- `go/file1c-telemetry` отдает `302` на Grafana
- SQLite Grafana подтвердил наличие `1c-file-telemetry` в folder `1C File Analytics`

## Незафиксированные в git риски

- коммитов за последние 24 часа нет;
- есть значительный dirty worktree за пределами этого отчета;
- часть новых файлов все еще `??` в `git status`;
- без отдельного git-упорядочивания следующий человек может смешать сегодняшние изменения с более старыми незакоммиченными правками.

## Следующий жесткий шаг

Если цель именно финансовая отчетность по проводкам, нужен отдельный read-only пользователь 1С для боевых файловых баз. Это единственный текущий внешний blocker, который не решается локальным refactor/deploy.
