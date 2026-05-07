# Установка на Windows

Установка ActivityWatch коллекторов на Windows рабочие станции.

## Требования

- Windows 10/11
- PowerShell 5.1+
- Административские права
- Доступ к ActivityWatch Server

## Автоматическая установка

### Через Domain Controller (GPO)

```powershell
# На Domain Controller
.\windows\deploy-domain-users.ps1
```

Этот скрипт:
1. Создает GPO для развертывания
2. Копирует коллекторы на рабочие станции
3. Настраивает scheduled tasks
4. Конфигурирует DLP политики

### Для одиночного пользователя

```powershell
# Локально на workstation
.\windows\deploy-single-user.ps1
```

## Ручная установка

### 1. Установка ActivityWatch

```powershell
# Скачайте installer с сайта ActivityWatch
# Запустите installer

# Или через InnoSetup installer
.\install-kit-awindows-*.exe
```

### 2. Копирование коллекторов

```powershell
# Создайте директорию
New-Item -ItemType Directory -Path "C:\ProgramData\AWatch-rus" -Force

# Скопируйте коллекторы
Copy-Item windows\dlp-endpoint-signals-collector.ps1 "C:\ProgramData\AWatch-rus\"
Copy-Item windows\browser-domains-native-collector.ps1 "C:\ProgramData\AWatch-rus\"
Copy-Item windows\email-outbound-collector.ps1 "C:\ProgramData\AWatch-rus\"
```

### 3. Настройка конфигурации

```powershell
# Создайте конфигурационный файл
@{
    aw_server_url = "http://aw-server:5600"
    bucket_prefix = "aw-watcher-dlp-endpoint"
    heartbeat_interval = 60
} | ConvertTo-Json | Out-File "C:\ProgramData\AWatch-rus\deployment-config.json"
```

### 4. Настройка Scheduled Task

```powershell
$trigger = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 5)
$action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-File C:\ProgramData\AWatch-rus\dlp-endpoint-signals-collector.ps1"
Register-ScheduledTask -TaskName "DLP Endpoint Collector" -Trigger $trigger -Action $action -RunLevel Highest
```

## Конфигурация DLP политик

### Создание файла политик

```powershell
@{
    clipboard_rules = @(
        @{
            pattern = "\b\d{4}-\d{4}-\d{4}-\d{4}\b"
            description = "Credit card numbers"
            severity = "high"
            action = "block"
        }
    )
    print_rules = @(
        @{
            printer_match = "*"
            document_keywords = @("confidential", "secret")
            action = "block"
        }
    )
} | ConvertTo-Json -Depth 10 | Out-File "C:\ProgramData\AWatch-rus\dlp-policy.json"
```

## Проверка установки

```powershell
# Проверка статуса службы
Get-ScheduledTask | Where-Object {$_.TaskName -like "*DLP*"}

# Проверка логов
Get-Content "C:\ProgramData\AWatch-rus\logs\*.log" -Tail 50

# Проверка соединения с сервером
Test-NetConnection -ComputerName aw-server -Port 5600
```

## Устранение проблем

### Коллектор не запускается
```powershell
# Проверьте execution policy
Get-ExecutionPolicy
Set-ExecutionPolicy RemoteSigned -Scope CurrentUser

# Проверьте права
# Запустите PowerShell от администратора
```

### Нет соединения с сервером
```powershell
# Проверьте firewall
New-NetFirewallRule -DisplayName "Allow AW Server" -Direction Outbound -Protocol TCP -RemotePort 5600 -Action Allow

# Проверьте URL в конфиге
Get-Content "C:\ProgramData\AWatch-rus\deployment-config.json"
```

### DLP не работает
```powershell
# Проверьте файл политик
Test-Path "C:\ProgramData\AWatch-rus\dlp-policy.json"

# Проверьте формат JSON
Get-Content "C:\ProgramData\AWatch-rus\dlp-policy.json" | ConvertFrom-Json
```

## Обновление

```powershell
# Остановите задачи
Unregister-ScheduledTask -TaskName "DLP Endpoint Collector" -Confirm:$false

# Обновите файлы
Copy-Item windows\dlp-endpoint-signals-collector.ps1 "C:\ProgramData\AWatch-rus\" -Force

# Перезапустите задачи
Register-ScheduledTask -TaskName "DLP Endpoint Collector" -Trigger $trigger -Action $action
```

## Удаление

```powershell
# Остановите задачи
Get-ScheduledTask | Where-Object {$_.TaskName -like "*DLP*"} | Unregister-ScheduledTask -Confirm:$false

# Удалите файлы
Remove-Item "C:\ProgramData\AWatch-rus" -Recurse -Force

# Удалите ActivityWatch (через Programs and Features)
```

## Подробнее

- [DLP Правила](DLP-Rules)
- [Компоненты](Components)
