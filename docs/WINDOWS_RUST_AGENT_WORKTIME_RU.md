# Windows Rust Agent Worktime/RDP

## Назначение

`awatch-agent-rs` заменяет PowerShell-сборщик
`worktime-session-collector.ps1` для учета Windows/RDP-сессий.

Цель перехода:

- меньше зависимость от PowerShell и локали Windows;
- стабильный сбор RDP/local/disconnected сессий через WinAPI WTS;
- единый `TelemetryRecord` для портала, отчетов, KPI активности и UEBA;
- сохранение PowerShell-сценария только как legacy fallback.

Агент не собирает содержимое окон, документов, ввод с клавиатуры или снимки
экрана. Для worktime/session path используются только:

- `username`;
- `session_id`;
- `session_type`;
- `active`;
- `started_at`, если платформа отдаст это поле;
- `remote_addr`, если платформа отдаст это поле.

## Конфигурация агента

Файл Windows:

```text
C:\ProgramData\AWatch-rus\agent\awatch-agent.toml
```

Минимальный пример:

```toml
server_url = "https://<GATEWAY_HOST>/api/telemetry"
api_key = "CHANGE_ME"
collect_interval_seconds = 30
role = "workstation"

enable_processes = true
enable_network = true
enable_security_events = true
enable_workforce_activity = true

spool_dir = "C:\\ProgramData\\AWatch-rus\\agent\\spool"
timeout_seconds = 10
retry_attempts = 3

aw_api_base = "http://<AW_SERVER_HOST>:5600/api/0"
aw_worktime_enabled = true
```

По умолчанию в example-конфиге `aw_worktime_enabled=false`. Включайте его
только после настройки `aw_api_base`, spool-директории и rollback-процедуры.

## Источники сессий

Агент пишет диагностическое поле `session_source`:

- `wts_api` - основной промышленный путь через WinAPI WTS;
- `quser_utf16` - fallback через `query user`/`quser` в UTF-16;
- `quser_lossy` - fallback через обычный console output;
- `env_sessionname_fallback` - fallback по переменной `SESSIONNAME`;
- `local_fallback` - последняя локальная заглушка, когда системные источники
  не вернули сессии.

В `TelemetryRecord.diagnostics` дополнительно пишутся:

- `sessions_collected_total`;
- `rdp_sessions_total`;
- `active_sessions_total`;
- `collector_source`;
- `collector_error`.

## Достоверность данных агента

Портал и отчеты поднимают diagnostics в два блока:

- `agent_quality` - обратная совместимость и технические счетчики;
- `agent_quality_explain` - управленческий вывод о доверии к KPI.

Статусы `agent_quality`:

- `ok` - основной источник `wts_api`, ошибки коллектора нет;
- `fallback` - данные получены через `quser_utf16`, `quser_lossy` или
  `env_sessionname_fallback`;
- `degraded` - используется `local_fallback` или есть некритичная ошибка
  коллектора;
- `error` - есть критичная ошибка коллектора, например отказ доступа,
  некорректный обязательный payload или ошибка парсинга;
- `unknown` - старый агент или payload без diagnostics.

Статусы `agent_quality_explain`:

- `OK` - данные приняты в KPI;
- `WARNING` - данные собраны резервным способом, допустимы как оперативный
  ориентир, но требуют проверки для доказательной базы;
- `DEGRADED` - данные не приняты в KPI;
- `UNKNOWN` - агент не передал диагностику качества данных.

`local_fallback` считается диагностическим сигналом, а не доказательством
активности. События worktime, опубликованные из `local_fallback`, получают
`active=false`, `ignoredForKpi=true` и не должны увеличивать KPI сотрудника или
подтверждать RDP-активность. В портале для этого режима выводится текст:
`Диагностический режим, данные не засчитываются в KPI`.

Портал также строит `agent_quality_history` за 7 дней из telemetry JSONL:
последняя запись каждого дня превращается в статус доверия к KPI. Если за
неделю меньше 5 дней `OK`, недельный KPI считается требующим валидации.

Портал строит `agent_quality_nodes` по последней записи каждого рабочего места
за период. Ключ узла выбирается так: `hostname`, затем `machine_id`, затем
`unknown`. Это позволяет не ломать старые telemetry payload и одновременно
показывать руководителю, какие рабочие станции портят достоверность KPI.
Источник `local_fallback` в этой сводке помечается как диагностический и не
подтверждает KPI узла.

На основе `agent_quality_nodes` портал также считает `agent_coverage_sla`:
покрытие ожидаемого парка рабочих мест по файлу expected nodes. Свежий
`local_fallback` может подтверждать наличие телеметрии, но не подтверждает KPI
узла и снижает `coverage_pct`.

## PowerShell Legacy Fallback

В `deployment-config.json` используется блок:

```json
{
  "collectors": {
    "worktimeSessionEnabled": true,
    "worktimeSessionMode": "rust_primary",
    "worktimeLegacyFallbackEnabled": true
  }
}
```

Режимы:

- `powershell_primary` - старое поведение, PowerShell collector основной;
- `rust_primary` - Rust agent основной, PowerShell запускается только как
  legacy fallback при недоступности Rust agent или stale worktime bucket.

Для полного отключения PowerShell worktime fallback:

```json
{
  "collectors": {
    "worktimeSessionEnabled": false,
    "worktimeSessionMode": "rust_primary",
    "worktimeLegacyFallbackEnabled": false
  }
}
```

## Проверка

Проверить локальный JSON агента:

```powershell
C:\ProgramData\AWatch-rus\agent\awatch-agent-rs.exe `
  --config C:\ProgramData\AWatch-rus\agent\awatch-agent.toml `
  --once --print-json
```

Ожидаемые признаки:

- `active_sessions` содержит local/RDP/disconnected сессии;
- `rdp_sessions` содержит активные RDP-сессии;
- `session_source` равен `wts_api` в штатном режиме;
- `diagnostics.rdp_sessions_total` соответствует числу RDP-сессий.

Проверить, что legacy PowerShell не запущен:

```powershell
Get-CimInstance Win32_Process |
  Where-Object {
    $_.Name -match 'powershell|pwsh' -and
    $_.CommandLine -match 'worktime-session-collector.ps1'
  }
```

Проверить ActivityWatch bucket:

```bash
curl "http://<AW_SERVER_HOST>:5600/api/0/buckets/aw-worktime-sessions_<HOST>/events?limit=5"
```

В свежих событиях должно быть:

```json
{
  "source": "awatch-agent-rs",
  "sessionSource": "wts_api",
  "collectorSource": "wts_api",
  "ignoredForKpi": false
}
```

## Spool и восстановление

Если `aw_worktime_enabled=true`, но `aw_api_base` временно недоступен, агент
кладет worktime-записи в:

```text
<spool_dir>\aw-worktime
```

При следующем успешном цикле агент пытается выгрузить накопленный spool с тем
же `retry_attempts`.

## Rollback на PowerShell

1. Остановить Rust Scheduled Task или сервис агента.
2. В `deployment-config.json` установить:

```json
{
  "collectors": {
    "worktimeSessionEnabled": true,
    "worktimeSessionMode": "powershell_primary",
    "worktimeLegacyFallbackEnabled": true
  }
}
```

3. Перезапустить guard/recovery AWatch-rus.
4. Проверить, что `worktime-session-collector.ps1` снова пишет события в
   `aw-worktime-sessions_<HOST>`.

PowerShell collector не удаляется из поставки именно для такого rollback.
