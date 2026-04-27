# Runbook

## Быстрый health-check

### На Proxmox

```sh
pct status <CT_ID>
pct config <CT_ID>
pct exec <CT_ID> -- systemctl is-active activitywatch-server.service
pct exec <CT_ID> -- curl -fsS http://127.0.0.1:5600/api/0/info
```

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
