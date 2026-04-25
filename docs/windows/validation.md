# Windows validation

## Автоматическая проверка (рекомендуется)

```powershell
$report = .\windows\validate-deployment.ps1 `
  -ConfigPath C:\ProgramData\ActivityWatch\deployment-config.json
$report | ConvertTo-Json -Depth 12
```

Критерий:

- `overallOk = true`

## Базовая проверка после развёртывания

### 1. Проверить установленные файлы

```powershell
Test-Path 'C:\Program Files\ActivityWatch\aw-watcher-afk\aw-watcher-afk.exe'
Test-Path 'C:\Program Files\ActivityWatch\aw-watcher-window\aw-watcher-window.exe'
Test-Path 'C:\ProgramData\ActivityWatch\browser-domains-native-collector.ps1'
Test-Path 'C:\ProgramData\ActivityWatch\dlp-policy.json'
Test-Path 'C:\ProgramData\ActivityWatch\deployment-config.json'
```

Ожидаемый результат — везде `True`.

### 2. Проверить задачи

```powershell
Get-ScheduledTask -TaskName 'ActivityWatch*' |
  Select-Object TaskName, Author, State
```

Ожидаемо:

- по одной задаче `ActivityWatch Launch [...]` на пользователя;
- одна задача `ActivityWatch Recovery`.

### 3. Проверить процессы после логина пользователя

```powershell
Get-Process aw-watcher-afk, aw-watcher-window -ErrorAction SilentlyContinue |
  Select-Object ProcessName, SessionId, StartTime
```

И collector:

```powershell
Get-CimInstance Win32_Process |
  Where-Object { $_.Name -in 'powershell.exe', 'pwsh.exe' -and $_.CommandLine -match 'browser-domains-native-collector.ps1' } |
  Select-Object ProcessId, SessionId, CommandLine
```

### 4. Проверить сетевую связность

```powershell
Test-NetConnection aw.example.local -Port 5600
Invoke-WebRequest http://aw.example.local:5600/api/0/info
```

Если используется HTTPS:

```powershell
Invoke-WebRequest https://aw.example.local/api/0/info
```

### 5. Проверить buckets на сервере

После открытия нескольких сайтов в браузере на сервере должны появиться:

- `aw-watcher-window_<hostname>`
- `aw-watcher-web-edge_<hostname>` или другой browser bucket
- `aw-detmir-web-category_<hostname>`
- `aw-dlp-incidents_<hostname>` (при срабатывании policy rule с `action=alert|block|quarantine`)

Проверка через API:

```powershell
Invoke-WebRequest http://aw.example.local:5600/api/0/buckets | Select-Object -ExpandProperty Content
```

## Проверка категоризации

1. Откройте сайт из кастомного правила.
2. Подождите `PollSeconds + PulseSeconds`.
3. Проверьте category bucket на сервере.
4. Убедитесь, что поля `domain`, `rootDomain`, `category`, `categoryGroup`, `categoryRule` заполнены.

## Проверка DLP phase-1

1. В `dlp-policy.json` задайте правило на тестовый домен.
2. Откройте этот домен в браузере.
3. Проверьте `aw-dlp-incidents_<hostname>` через API.
4. Проверьте локальный лог:

```powershell
Get-Content "C:\ProgramData\ActivityWatch\logs\dlp-incidents-$env:USERNAME.log" -Tail 50
```

## Проверка восстановления

1. Завершите `aw-watcher-afk.exe` и `aw-watcher-window.exe` у тестового пользователя.
2. Подождите до `RecoveryIntervalSeconds`.
3. Убедитесь, что per-user launch task стартовала процессы заново.

Ручной запуск recovery:

```powershell
Start-ScheduledTask -TaskName 'ActivityWatch Recovery'
```

## Проверка ACL

```powershell
icacls 'C:\Program Files\ActivityWatch'
icacls 'C:\ProgramData\ActivityWatch'
icacls 'C:\ProgramData\ActivityWatch\logs'
```

Ожидаемо:

- `SYSTEM` и `Administrators` имеют `F`;
- `Users` имеет `RX` на install/state;
- `Users` имеет `M` на `logs`.

## Критерий готовности к массовому развёртыванию

- Установка проходит без ручного редактирования скриптов.
- Все параметры инфраструктуры передаются снаружи.
- Повторный запуск не ломает текущую установку.
- Recovery восстанавливает запуск watcher'ов после остановки.
- Collector пишет domain/category события без расширений браузера.
