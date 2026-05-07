# Runbook

## Быстрый health-check

### На Proxmox

```sh
pct status <CT_ID>
pct config <CT_ID>
pct exec <CT_ID> -- systemctl is-active activitywatch-server.service
pct exec <CT_ID> -- curl -fsS http://127.0.0.1:5600/api/0/info
```

### На host-based инсталляции

Для подтвержденного размещения на `10.10.10.2`:

```sh
ps -ef | grep -E 'aw-server-rust|pfsense-aw-poller' | grep -v grep
curl -fsS http://127.0.0.1:5600/api/0/info
ss -ltnp | grep 5600
```

Ожидаемо должны быть видны:

- `aw-server-rust` с `--webpath /opt/aw-webui-ru`;
- `pfsense-aw-poller.py --config /etc/aw-pfsense/poller.json`.

### Внутри CT

```sh
systemctl status activitywatch-server.service --no-pager
journalctl -u activitywatch-server.service -n 100 --no-pager
curl -fsS http://127.0.0.1:5600/api/0/info
ss -ltnp | grep 5600
```

## Проверка RU patch

```sh
grep -n 'aw-ru-patch\|aw-sw-cleanup' /opt/activitywatch/webui-ru/index.html
ls -l /opt/activitywatch/webui-ru/js/
```

Проверить:

- есть `aw-ru-patch.js`;
- есть `aw-sw-cleanup.js`;
- `index.html` содержит оба include;
- `service-worker.js` заменён cleanup-версией.

## Рабочее время (worktime)

Если в деплое включен `aw_apply_worktime_settings: true`, playbook применяет базовую категоризацию (classes) и view `worktime`.

Контрольные AQL-шаблоны для расчета рабочего времени: `docs/worktime_aql_detmir.md`.

Период рабочего времени задаётся переменными:

- `aw_worktime_from` (например `08:00`)
- `aw_worktime_to` (например `17:00`)

Playbook вычисляет `durationDefault` автоматически (включая смены через полночь) и выставляет:
`/api/0/settings/startOfDay` и `/api/0/settings/durationDefault`.

## Типовые инциденты

### DLP не виден в вебе

Быстрый чек сервера:

```sh
curl -fsS http://127.0.0.1:5600/api/0/buckets | jq -r 'keys[] | select(test("^aw-dlp-"))'
curl -fsS http://127.0.0.1:5600/api/0/buckets/aw-dlp-incidents_SHARKON2025 | jq '{end:.metadata.end}'
```

Контролируемый тест ingest:

```sh
TS=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
PAYLOAD=$(jq -nc --arg ts "$TS" '{timestamp:$ts,duration:0,data:{ruleId:"selftest-dlp-incident",action:"alert",severity:"low",message:"Self-test DLP incident from runbook",signalType:"self_test",username:"AUTOTEST",sessionId:0,hostname:"SHARKON2025",source:"self-test"}}')
curl -fsS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-incidents_SHARKON2025/heartbeat?pulsetime=60' -H 'Content-Type: application/json' --data "$PAYLOAD"
curl -fsS 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-incidents_SHARKON2025/events?limit=5' | jq '.[0].data'
```

Если API видит событие, а bucket-страница в UI показывает старые `First/last event`, нажать `Обновить` на странице bucket и раскрыть `Events`.

### У пользователей всплывает окно PowerShell

Ожидаемое поведение collector-ов: запуск hidden (`-WindowStyle Hidden`).
Проверка на Windows хосте:

```powershell
Get-CimInstance Win32_Process |
  Where-Object { $_.CommandLine -and ($_.CommandLine -match 'browser-domains-native-collector.ps1' -or $_.CommandLine -match 'dlp-endpoint-signals-collector.ps1') } |
  Select-Object SessionId, ProcessId, CommandLine
```

Если нужно экстренно убрать снимки инцидентов:

1. Поставить `incidentCapture.screenshotEnabled = false` в `deployment-config.json` (для каждого StateRoot).
2. Запустить `Start-ScheduledTask -TaskName 'ActivityWatch Recovery'`.

### SHARKON2025: `Активное время = 0s`, хотя `window`-события есть

Симптом:

- в Activity view за день видно `Worktime = 0s`;
- `Top Window Titles / Top Categories / Category Tree` пустые;
- при этом bucket `aw-watcher-window_SHARKON2025` содержит свежие события.

Подтвержденная причина:

- watcher `afk` "залип" в `status=afk` без `not-afk`;
- из-за этого дневная сводка не считает подтвержденную активность.

Быстрый recovery (с Linux admin host):

1. Проверить учетку входа. Для этого кейса рабочая учетная запись: `SHARKON2025\Администратор` (не `Administrator`).
2. Поднять remote execution через `wmiexec.py` с auth-file:

```sh
cat > /tmp/sharkon_ru.auth << 'EOF'
username = Администратор
password = <PASSWORD>
domain = SHARKON2025
EOF
chmod 600 /tmp/sharkon_ru.auth
```

3. Запустить recovery task:

```sh
wmiexec.py -nooutput -A /tmp/sharkon_ru.auth 192.168.100.21 \
  "powershell -NoProfile -Command \"Start-ScheduledTask -TaskName 'ActivityWatch Recovery'\""
```

4. Запустить все launch tasks:

```sh
wmiexec.py -nooutput -A /tmp/sharkon_ru.auth 192.168.100.21 \
  "powershell -NoProfile -Command \"Get-ScheduledTask | Where-Object TaskName -like 'ActivityWatch Launch *' | ForEach-Object { Start-ScheduledTask -TaskName \$_.TaskName }\""
```

5. Подождать 10-20 секунд и проверить API на AW server (`10.10.10.13:5600`):

```sh
curl -fsS 'http://10.10.10.13:5600/api/0/buckets/aw-watcher-afk_SHARKON2025/events?limit=30' \
  | jq '{latest:.[0].timestamp, statuses:(group_by(.data.status)|map({status:.[0].data.status,count:length}))}'
```

Ожидаемо после фикса:

- в свежих AFK-событиях появляется `status=not-afk`;
- `aw-watcher-window_SHARKON2025` продолжает обновляться;
- после обновления страницы UI дневная сводка перестает быть `0s`.

### Сервис не стартует

```sh
systemctl cat activitywatch-server.service
cat /etc/activitywatch/aw-server.env
journalctl -xeu activitywatch-server.service --no-pager
```

Частые причины:

- битый URL релиза;
- неполная распаковка архива;
- занят порт;
- не созданы каталоги или пользователь;
- ошибка в env-файле.

### API отвечает, но UI без русификации

Проверить:

- патч реально вставлен в `index.html`;
- browser cache/service worker очищен;
- reverse proxy не отдаёт старую статику;
- сервис был перезапущен после правок.

Повторное применение:

```sh
bash /root/bootstrap/apply_webui_ru_patch.sh
systemctl restart activitywatch-server.service
```

### После обновления UI сломался патч

- сравнить `index.html` с backup;
- заново применить patch script;
- проверить словарь в `aw-ru-patch.js`;
- при необходимости откатить только Web UI override.

## Перед любыми изменениями

0. Подтвердить maintenance window и gate:

```sh
export AW_MAINTENANCE_ACK=YES
./scripts/quality-gate.sh
```

Если `quality-gate` падает (например, drift install-kit vs repo), rollout не запускать.

1. Сделать snapshot или `vzdump`.
2. Сохранить текущий `/etc/activitywatch/aw-server.env`.
3. Сохранить текущий `index.html`.
4. Зафиксировать текущую версию `aw-server-rust`.

## Критерии готовности

- systemd unit стартует без ручного вмешательства;
- API `/api/0/info` отвечает локально;
- UI открывается;
- русификация присутствует;
- rollback-путь понятен оператору.
