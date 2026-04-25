# Windows troubleshooting

## Частые проблемы

### `Run this script from an elevated PowerShell session`

- Откройте PowerShell через `Run as administrator`.
- Проверьте, что UAC не понизил токен.

### Задачи созданы, но watcher'ы не стартуют

- Убедитесь, что пользователь реально вошёл в интерактивную сессию.
- Проверьте задачу `ActivityWatch Launch [...]` в Task Scheduler.
- Запустите вручную:

```powershell
Start-ScheduledTask -TaskName 'ActivityWatch Launch [CONTOSO_user01]'
```

- Если доменный логон ещё ни разу не происходил на хосте, сначала выполните вход этим пользователем.

### Collector не видит URL

- Скрипт работает через UI Automation и foreground window.
- Некоторые браузеры/страницы могут скрывать адресную строку или блокировать UIA.
- Проверьте лог `C:\ProgramData\ActivityWatch\logs\browser-domains-<user>.log`.
- Убедитесь, что активное окно — поддерживаемый браузер: Edge, Chrome, Brave, Vivaldi, Opera, Firefox.

### Сервер недоступен

- Проверьте TCP-доступ:

```powershell
Test-NetConnection aw.example.local -Port 5600
```

- Проверьте локально API:

```powershell
Invoke-WebRequest http://aw.example.local:5600/api/0/info
```

- Если нужен HTTPS reverse proxy, задайте `-ServerScheme https`.

### Неправильная категоризация домена

- Проверьте содержимое `C:\ProgramData\ActivityWatch\web-category-rules.json`.
- Пользовательские правила должны быть валидным JSON.
- Один и тот же домен лучше определять только в одной категории.
- После изменения правил достаточно перезапустить collector или задачу пользователя:

```powershell
Stop-Process -Name powershell -ErrorAction SilentlyContinue
Start-ScheduledTask -TaskName 'ActivityWatch Launch [CONTOSO_user01]'
```

### ACL сломаны или пользователи удалили runtime-файлы

- Запустите:

```powershell
.\windows\hardening-recovery.ps1 -RepairPackage
```

- Если сервер и пользователи уже есть в `deployment-config.json`, дополнительные параметры не нужны.

## Диагностика

Проверить задачи:

```powershell
Get-ScheduledTask -TaskName 'ActivityWatch*' | Select-Object TaskName, State
```

Проверить процессы в пользовательской сессии:

```powershell
Get-Process aw-watcher-afk, aw-watcher-window -ErrorAction SilentlyContinue |
  Select-Object ProcessName, Id, SessionId, StartTime
```

Проверить collector:

```powershell
Get-CimInstance Win32_Process |
  Where-Object { $_.Name -in 'powershell.exe', 'pwsh.exe' -and $_.CommandLine -match 'browser-domains-native-collector.ps1' } |
  Select-Object ProcessId, SessionId, CommandLine
```

Проверить конфиг:

```powershell
Get-Content C:\ProgramData\ActivityWatch\deployment-config.json -Raw
```

## Когда запускать hardening/recovery

- После ручной чистки задач.
- После неудачного обновления ActivityWatch.
- После переноса сервера на другой host/port.
- После обновления списка пользователей.
- После изменения кастомных правил, если нужен централизованный repair pass.
