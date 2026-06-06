# Агрегированные события безопасности через ClickHouse

## Назначение

`detmir-portal` может показывать краткую сводку событий безопасности за последние
24 часа из ClickHouse. Это дополнительный аналитический слой для управленческого
вывода, рисков подразделений, карты рисков и связи рисков с активностью.

Важно: этот режим не превращает AWatch-rus в SIEM. Портал не показывает сырые
журналы, не хранит SIEM-события как первичный источник и не создает инциденты
автоматически. В интерфейсе отображаются только агрегаты.

## Статус по умолчанию

Режим выключен:

```env
SECURITY_EVENTS_BACKEND=disabled
```

В выключенном режиме портал работает как раньше, `/api/reports` возвращает
валидный JSON, а `detmir-check` не проверяет ClickHouse.

## Переменные окружения

```env
SECURITY_EVENTS_BACKEND=clickhouse
CLICKHOUSE_URL=http://127.0.0.1:8123
CLICKHOUSE_DATABASE=analytics_1c
CLICKHOUSE_USER=default
CLICKHOUSE_PASSWORD=
```

`CLICKHOUSE_PASSWORD` используется только для HTTP Basic Auth и не выводится в
JSON, markdown-отчеты или health-ответы.

## Текущий DetMir runtime

В рабочем контуре DetMir ClickHouse запущен не на AW-сервере `<AW_SERVER_HOST>`, а
на gateway/Proxmox-хосте `<GATEWAY_HOST>` как Docker Compose service:

- каталог: `/opt/activitywatch/clickhouse-1c`;
- контейнер: `aw-rus-1c-clickhouse`;
- образ: `clickhouse/clickhouse-server:24.8`;
- HTTP: `<GATEWAY_HOST>:8123`;
- native: `<GATEWAY_HOST>:9000`;
- база: `analytics_1c`;
- credentials: `/opt/activitywatch/clickhouse-1c/.env` на `<GATEWAY_HOST>`.

Для `detmir-portal` или `detmir-check`, запущенных на AW-сервере
`<AW_SERVER_HOST>`, используйте:

```env
SECURITY_EVENTS_BACKEND=clickhouse
CLICKHOUSE_URL=http://<GATEWAY_HOST>:8123
CLICKHOUSE_DATABASE=analytics_1c
CLICKHOUSE_USER=default
CLICKHOUSE_PASSWORD=<из /opt/activitywatch/clickhouse-1c/.env на <GATEWAY_HOST>>
```

Проверка с `<AW_SERVER_HOST>` без пароля может вернуть `AUTHENTICATION_FAILED`; это
подтверждает сетевую доступность `<GATEWAY_HOST>:8123`, но не проверяет
аутентификацию.

## Контроль состояния

Текущий контур контроля ClickHouse:

- Docker Compose healthcheck у контейнера `aw-rus-1c-clickhouse`. В норме
  `docker compose ps` показывает статус `(healthy)`.
- `aw-1c-clickhouse-health.timer` на `<GATEWAY_HOST>`, запуск каждые 5 минут.
  Проверяет Docker state, Docker health, authenticated `SELECT 1` через
  `clickhouse-client`, HTTP `SELECT 1`, freshness таблиц и свободное место на
  volume ClickHouse.
- `aw-clickhouse-network-health.timer` на AW-сервере `<AW_SERVER_HOST>`, запуск
  каждые 5 минут. Проверяет TCP-доступность `<GATEWAY_HOST>:8123/9000` и HTTP-ответ
  ClickHouse со стороны AW-rus сервера.
- `aw-1c-ingest.timer` сохранен как writer в ClickHouse: цикл сбора и записи
  данных выполняется раз в 15 минут (`OnUnitActiveSec=15min`) через Rust-бинарник
  `/usr/local/bin/aw-1c-ingest-rust`.
- Windows-задача `ActivityWatch File1C Upload` на RDP-хосте запускает
  Rust-бинарник `C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe`
  в режиме `file1c-upload`; legacy PowerShell exporter оставлен только как
  fallback.
- DLP-события обрабатываются не реже 15 минут: `activitywatch-dlp-aggregator`
  работает каждые 5 минут, `aw-dlp-influx-exporter` - каждые 10 минут,
  CEF/syslog/webhook forwarder'ы - каждые 2-5 минут, Windows
  `ActivityWatch DLP Evidence Sync` запускает тот же Rust-бинарник в режиме
  `dlp-evidence-sync` каждые 15 минут. Sync копирует только PNG, похожие на
  DLP incident screenshots (`web`, `clipboard`, `usb_insert`, `print_job` в
  имени файла), и игнорирует 1C/прочие PNG в `incident-artifacts`.
- `aw-1c-proofcheck.timer` отдельно проверяет свежесть 1C-таблиц.

Операционные команды:

```bash
ssh detmir_proxmox 'cd /opt/activitywatch/clickhouse-1c && sudo docker compose ps'
ssh detmir_proxmox 'systemctl status aw-1c-clickhouse-health.timer aw-1c-clickhouse-health.service --no-pager'
ssh detmir_proxmox 'sudo journalctl -u aw-1c-clickhouse-health.service -n 30 --no-pager'
ssh detmir_aw 'systemctl status aw-clickhouse-network-health.timer aw-clickhouse-network-health.service --no-pager'
ssh detmir_aw 'sudo journalctl -u aw-clickhouse-network-health.service -n 30 --no-pager'
```

## Ручное изменение параметров Windows DLP evidence sync

Использовать, если нужно временно изменить период или API без полного redeploy.
После ручного изменения желательно перенести значение в Ansible vars.

### Период DLP evidence sync

На Windows/RDP host в elevated PowerShell:

```powershell
$taskName = "ActivityWatch DLP Evidence Sync"
$minutes = 15

$task = Get-ScheduledTask -TaskName $taskName
$trigger = New-ScheduledTaskTrigger -Once -At ((Get-Date).Date) `
  -RepetitionInterval (New-TimeSpan -Minutes $minutes) `
  -RepetitionDuration (New-TimeSpan -Days 3650)

Set-ScheduledTask -TaskName $taskName `
  -Action $task.Actions `
  -Trigger $trigger `
  -Principal $task.Principal `
  -Settings $task.Settings
```

### Evidence API / token / state / log paths

```powershell
$exe = "C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe"
$args = 'dlp-evidence-sync --evidence-api-url "http://<AW_SERVER_HOST>:8721/api/dlp/evidence/upload" --token-path "C:\ProgramData\AWatch-rus\dlp-evidence-upload-token.txt" --state-path "C:\ProgramData\AWatch-rus\dlp-evidence-sync-state.json" --log-path "C:\ProgramData\AWatch-rus\logs\dlp-evidence-sync.log"'

$action = New-ScheduledTaskAction -Execute $exe -Argument $args
Set-ScheduledTask -TaskName "ActivityWatch DLP Evidence Sync" -Action $action
```

Проверить:

```powershell
& "C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe" dlp-evidence-sync `
  --evidence-api-url "http://<AW_SERVER_HOST>:8721/api/dlp/evidence/upload" `
  --token-path "C:\ProgramData\AWatch-rus\dlp-evidence-upload-token.txt" `
  --state-path "C:\ProgramData\AWatch-rus\dlp-evidence-sync-state.json" `
  --log-path "C:\ProgramData\AWatch-rus\logs\dlp-evidence-sync.log"

Get-ScheduledTaskInfo -TaskName "ActivityWatch DLP Evidence Sync"
```

## Ожидаемые агрегаты

Портал формирует блок `security_events_summary`:

- `status`;
- `backend`;
- `events_24h`;
- `failed_logins_24h`;
- `suspicious_logins_24h`;
- `rdp_sessions_24h`;
- `account_changes_24h`;
- `agent_errors_24h`;
- `top_departments`;
- `last_event_utc`;
- `query_ms`;
- `fallback_used`.

Текущая реализация использует агрегированные запросы к таблицам
`entity_timeline` и `host_events` выбранной базы ClickHouse.

## Поведение при отказе

Если `SECURITY_EVENTS_BACKEND=clickhouse`, но ClickHouse недоступен:

- портал не падает;
- `/api/reports` остается валидным;
- `security_events_summary.fallback_used=true`;
- в ролевом представлении “Эксплуатация” показывается причина;
- `detmir-check` добавляет необязательное предупреждение, но не считает это
  критическим отказом контура.

## Где видно в портале

- “Сводка руководителя”: короткий счетчик событий за 24 часа.
- “Главный вывод”: события учитываются как один из подтверждающих слоев риска.
- “Риски подразделений”: события повышают приоритет подразделения только как
  агрегированный риск-фактор.
- “Карта рисков”: события отображаются отдельной колонкой.
- “Связь рисков и активности”: события участвуют в объяснении корреляции.
- “ИБ”: блок “События безопасности за 24 часа”.
- “Эксплуатация”: статус источника и причина fallback.

## Проверка

Выключенный режим:

```bash
SECURITY_EVENTS_BACKEND=disabled detmir-check --json
curl -s http://127.0.0.1:8720/portal/api/reports | jq '.security_events_summary'
```

Включенный режим:

```bash
SECURITY_EVENTS_BACKEND=clickhouse \
CLICKHOUSE_URL=http://127.0.0.1:8123 \
CLICKHOUSE_DATABASE=analytics_1c \
detmir-check --json
```

Ожидаемый результат при доступном ClickHouse: `security-events-clickhouse` имеет
`ok=true`, а `/api/reports.security_events_summary.backend="clickhouse"`.

Ожидаемый результат при недоступном ClickHouse: портал возвращает
`fallback_used=true`, а `detmir-check` показывает предупреждение
`security-events-clickhouse` с `required=false`.

## Пилотная проверка UI

Для демонстрационного стенда проверяются три режима:

1. `SECURITY_EVENTS_BACKEND=disabled` - штатный режим без ClickHouse.
2. `SECURITY_EVENTS_BACKEND=clickhouse` и доступный ClickHouse - сводка
   показывает “События безопасности доступны”.
3. `SECURITY_EVENTS_BACKEND=clickhouse` и недоступный ClickHouse - портал
   показывает “События безопасности временно недоступны”, но `/api/reports`
   остается валидным.

Smoke-тест поддерживает явное ожидание режима:

```bash
DETMIR_PORTAL_SMOKE_SECURITY_EVENTS_EXPECT=disabled \
node scripts/detmir-portal-tabs-smoke.mjs

DETMIR_PORTAL_SMOKE_SECURITY_EVENTS_EXPECT=fallback \
node scripts/detmir-portal-tabs-smoke.mjs
```

В представлении руководителя не выводятся технические параметры
`SECURITY_EVENTS_BACKEND` и `CLICKHOUSE_*`. Подробная причина отказа видна в
представлении эксплуатации.
