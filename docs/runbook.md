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

### Расширенный DLP transport health-check

На AW-server:

```sh
/usr/local/bin/aw-health-check
```

Что проверяет дополнительно:
- свежесть DLP bucket-ов (`aw-dlp-endpoint-signals_*`, `aw-file-operations_*`);
- наличие transport/self-test telemetry (`queueDepth`, `eventsEnqueued`, `eventsFlushed`, `sendFailures`) в endpoint self-test;
- API-доступность базовых сервисов.

Интерпретация:
- `FAIL` — есть критичная проблема (service/API/stale transport);
- `WARN` — сигнал для оператора (например, bucket еще не активирован на хосте), но без hard-fail.

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

- `aw_worktime_from` (например `00:00`)
- `aw_worktime_to` (например `17:00`)

Playbook вычисляет `durationDefault` автоматически (включая смены через полночь) и выставляет:
`/api/0/settings/startOfDay` и `/api/0/settings/durationDefault`.

## Типовые инциденты

### Hayabusa: production validation end-to-end

Цель: подтвердить один реальный путь

- Windows EVTX export
- перенос пакета на `10.10.10.13`
- intake через `aw-hayabusa`
- генерация отчёта
- привязка bounded metadata к операторскому follow-up

Минимальный production-proven сценарий:

1. На Windows-хосте сделать экспорт:

```powershell
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1 -DaysBack 1
```

2. Проверить, что пакет реально появился:

```powershell
Get-ChildItem 'C:\ProgramData\AWatch-rus\forensics\evtx-exports' |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 5 Name,Length,LastWriteTime
```

Ожидаемо должен появиться zip вида:

- `HOST-YYYYMMDD-HHMMSS.zip`

3. Перенести zip на `10.10.10.13` в операторскую рабочую зону.

4. На `AW-server` проверить раннер:

```bash
aw-hayabusa doctor
aw-hayabusa inventory
aw-hayabusa profiles
```

5. Принять пакет:

```bash
aw-hayabusa accept --package /path/to/HOST-YYYYMMDD-HHMMSS.zip --host HOST
```

6. Обработать inbox:

```bash
aw-hayabusa process-inbox --mode incident
```

7. Проверить результат:

```bash
aw-hayabusa inventory
readlink -f /opt/hayabusa/state/latest-run
readlink -f /opt/hayabusa/state/latest-HOST
find /opt/hayabusa/reports/HOST -maxdepth 2 -type f | sort
```

Ожидаемо должны быть:

- `summary.html`
- `manifest.json`
- `run.log`
- `timeline.jsonl` или `timeline.csv`
- `logon-summary-*.csv`

8. Проверить traceability:

```bash
cat /opt/hayabusa/state/latest-intake.json
find /opt/hayabusa/archive/packages/HOST -maxdepth 1 -type f | sort
find /opt/hayabusa/archive/extracted/HOST -maxdepth 2 -type f | sort
```

Нужно зафиксировать:

- `host`
- `intake_id`
- `sha256`
- `package_path`
- `report_dir`
- `status`

9. Для AW-rus/operator follow-up заносить только bounded metadata:

- `tool=hayabusa`
- `host`
- `mode=incident`
- `status`
- `intake_id`
- `package_path`
- `sha256`
- `report_dir`
- `summary_html`
- `timeline_path`
- `manifest_path`

Не заносить в case / comments:

- сырые EVTX
- полный Sigma output
- полный timeline body

10. Если прогона не получилось, записать tuning backlog:

- пустой/битый zip
- нет EVTX в payload
- слабый audit scope на Windows
- шумный или слишком тяжёлый результат
- неочевидная трассировка от пакета к отчёту

Acceptance для этого сценария:

- есть хотя бы один реальный zip-пакет;
- `aw-hayabusa` провёл intake и analysis без ручной импровизации;
- артефакты трассируются от `HOST` до `report_dir`;
- follow-up не тащит сырые forensic данные в обычные AW buckets.

Known-good live proof `2026-05-21`:

- `host=SHARKON2025`
- `case_id=30`
- `intake_id=20260521T125653Z_SHARKON2025-phase17-rerun3`
- `sha256=e86b9abbfc1d706ac706c6c8a89509ab17023344c50880641e9175f73f1198d4`
- `report_dir=/opt/hayabusa/reports/SHARKON2025/20260521T125654Z_incident_20260521T125653Z_SHARKON2025-phase17-rerun3`
- `latest-intake.json` status: `ok`
- AW-rus case linkage stored under `forensics.hayabusa`

Что реально нашли в production validation:

- Windows zip с backslash path separators давал `unzip` warning rc=1; wrapper не должен валить intake на таком предупреждении.
- timeline режимы должны использовать `rules/config`, а не корень rules directory.

После live proof держать как regression checks:

```bash
aw-hayabusa doctor
aw-hayabusa inventory
cat /opt/hayabusa/state/latest-intake.json
readlink -f /opt/hayabusa/state/latest-run
```

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

### Проверка WAL failover (Windows collectors)

Цель: подтвердить, что при недоступности AW API события не теряются, а буферизуются в локальной очереди и автоматически отправляются после восстановления связи.

На RDP-хосте (`192.168.100.18`) в PowerShell под администратором:

1) Проверить/обнулить очереди:

```powershell
$q1 = 'C:\ProgramData\AWatch-rus\file-operations-queue.jsonl'
$q2 = 'C:\ProgramData\AWatch-rus\dlp-endpoint-signals-queue.jsonl'
Get-Item $q1,$q2 | Select Name,Length,LastWriteTime
```

2) Временно заблокировать исходящий доступ на AW API (`:5600`):

```powershell
New-NetFirewallRule -DisplayName 'AWatch WAL Test Block 5600' -Direction Outbound -Action Block -Protocol TCP -RemotePort 5600 -ErrorAction SilentlyContinue
Enable-NetFirewallRule -DisplayName 'AWatch WAL Test Block 5600'
```

3) Сгенерировать тест-событие `file-operations`:

```powershell
$p = Join-Path $env:USERPROFILE 'Desktop\aw_wal_test.txt'
Set-Content -LiteralPath $p -Value ('wal-test ' + (Get-Date -Format o))
Start-Sleep -Seconds 5
```

4) Убедиться, что очередь выросла:

```powershell
Get-Item $q1,$q2 | Select Name,Length,LastWriteTime
```

5) Снять блокировку и дождаться flush:

```powershell
Disable-NetFirewallRule -DisplayName 'AWatch WAL Test Block 5600'
Start-Sleep -Seconds 20
Get-Item $q1,$q2 | Select Name,Length,LastWriteTime
```

Ожидаемо:
- на шаге 4 длина как минимум одного queue-файла увеличивается;
- на шаге 5 очередь уменьшается (в идеале до `0` или близко к фоновому уровню).

6) Проверка на AW server:

```sh
curl -fsS 'http://10.10.10.13:5600/api/0/buckets/aw-file-operations_10.10.10.13/events?limit=10' | jq '.[0].data'
```

После теста удалить правило:

```powershell
Remove-NetFirewallRule -DisplayName 'AWatch WAL Test Block 5600' -ErrorAction SilentlyContinue
```

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
wmiexec.py -nooutput -A /tmp/sharkon_ru.auth 192.168.100.18 \
  "powershell -NoProfile -Command \"Start-ScheduledTask -TaskName 'ActivityWatch Recovery'\""
```

4. Запустить все launch tasks:

```sh
wmiexec.py -nooutput -A /tmp/sharkon_ru.auth 192.168.100.18 \
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

## Аудит CryptoPro и готовности подписантов

Для повторяемой проверки сертификатов подписантов и встроенных лицензий CryptoPro:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/ansible
ansible-playbook -i inventory.ini audit_cryptopro_windows.yml
```

Итоговый JSON-отчёт сохраняется локально в:

```text
/tmp/aw-rus-cryptopro-audit-<user>/rdp-prod-cryptopro-audit.json
```

Отчёт содержит матрицу по профилям:

- `requestedUser`
- `profileUser`
- `thumbprint`
- `subject`
- `hasPrivateKey`
- `embeddedLicenseOk`
- `embeddedLicenseStatus`
- `container` если виден
- `actionNeeded`
