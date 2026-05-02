# Windows deployment

## Состав пакета

- `windows/deploy-single-user.ps1` — развёртывание для одного пользователя.
- `windows/deploy-domain-users.ps1` — массовое развёртывание по списку пользователей.
- `windows/deploy-ensemble.ps1` — orchestration-скрипт полного цикла (deploy + hardening + validation).
- `windows/hardening-recovery.ps1` — повторная регистрация задач, ACL и recovery-loop.
- `windows/validate-deployment.ps1` — машинная проверка состояния и JSON-отчёт.
- `windows/browser-domains-native-collector.ps1` — native collector доменов браузера с категоризацией.
- `windows/dlp-endpoint-signals-collector.ps1` — Windows/RDP collector (clipboard/USB/print signals).
- `windows/web-category-rules.example.json` — пример кастомных правил категоризации.
- `windows/dlp-policy.example.json` — пример DLP-политики (phase-1: alerting incidents).

## Что делает пакет

- Ставит `aw-watcher-afk` и `aw-watcher-window` из официального Windows ZIP ActivityWatch.
- Копирует browser-domain collector в `C:\ProgramData\AWatch-rus`.
- Копирует DLP policy в `C:\ProgramData\AWatch-rus\dlp-policy.json`.
- Включает `incidentCapture` в `deployment-config.json` для DLP-инцидентов:
  - `incidentCapture.screenshotEnabled = true`
  - `incidentCapture.artifactsRoot = <StateRoot>\incident-artifacts`
- Создаёт per-user задачи `ActivityWatch Launch [...]` с запуском при логоне.
- Создаёт системную задачу `ActivityWatch Recovery`, которая циклически перезапускает per-user launch tasks.
- Применяет ACL к `C:\Program Files\AWatch-rus\bin`, `C:\ProgramData\AWatch-rus` и каталогу логов.
- Не содержит хардкодов инфраструктуры: сервер, домен, список пользователей и правила передаются параметрами.
- Корректно регистрирует задачи через `-LogonType Interactive` (совместимо с Windows Server, где `InteractiveToken` не поддерживается).
- Поддерживает отключение шумных watcher'ов через `-AfkEnabled:$false` и `-WindowEnabled:$false`.

Важно:

- Скриншот делается только при DLP-инциденте (`Send-DlpIncidentHeartbeat`), не по таймеру и не на обычной активности.
- Для экстренного отключения снимков можно выставить `incidentCapture.screenshotEnabled = false` в `deployment-config.json` и запустить `ActivityWatch Recovery`.

## Предпосылки

- Windows 10/11 или Windows Server с PowerShell 5.1+.
- Запуск из elevated PowerShell.
- Доступ до ActivityWatch Server по `host:port`.
- Пользователи домена должны реально входить на хост интерактивно.
- Если интернет недоступен, заранее скачайте ActivityWatch ZIP и передайте `-PackageZipPath`.

## Single-user deploy

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope Process

.\windows\deploy-single-user.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -TargetUser 'CONTOSO\svc.activity.user01' `
  -CustomRulesPath .\windows\web-category-rules.example.json `
  -CustomPolicyPath .\windows\dlp-policy.example.json
```

Чтобы убрать `afkstatus` из веб-интерфейса, отключите AFK watcher:

```powershell
.\windows\deploy-single-user.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -TargetUser 'CONTOSO\svc.activity.user01' `
  -AfkEnabled:$false `
  -CustomRulesPath .\windows\web-category-rules.example.json `
  -CustomPolicyPath .\windows\dlp-policy.example.json
```

Если нужен только `window`-сигнал без AFK:

```powershell
.\windows\deploy-single-user.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -TargetUser 'CONTOSO\svc.activity.user01' `
  -AfkEnabled:$false `
  -WindowEnabled:$true `
  -CustomRulesPath .\windows\web-category-rules.example.json `
  -CustomPolicyPath .\windows\dlp-policy.example.json
```

Локальный пользователь:

```powershell
.\windows\deploy-single-user.ps1 `
  -ServerHost aw-gateway.internal `
  -ServerPort 5600 `
  -TargetUser '.\operator01'
```

Оффлайн-установка из локального ZIP:

```powershell
.\windows\deploy-single-user.ps1 `
  -ServerHost aw.example.local `
  -TargetUser 'CONTOSO\user01' `
  -PackageZipPath C:\Temp\activitywatch-v0.13.2-windows-x86_64.zip
```

## Multi-user / domain deploy

Поддерживаются:

- `-Users user1,user2`
- `-Users 'CONTOSO\user1','CONTOSO\user2'`
- `-UserListPath .\users.txt`
- `-UserListPath .\users.csv`

TXT-формат:

```text
# comments are ignored
user01
user02
user03
```

CSV-формат: колонка `User`, `Username`, `SamAccountName` или `Login`.

Пример:

```powershell
.\windows\deploy-domain-users.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -Domain CONTOSO `
  -UserListPath C:\Temp\aw-users.txt `
  -CustomRulesPath C:\Temp\web-category-rules.json `
  -CustomPolicyPath C:\Temp\dlp-policy.json
```

Для quiet-профиля без `afkstatus`:

```powershell
.\windows\deploy-domain-users.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -Domain CONTOSO `
  -UserListPath C:\Temp\aw-users.txt `
  -AfkEnabled:$false `
  -CustomRulesPath C:\Temp\web-category-rules.json `
  -CustomPolicyPath C:\Temp\dlp-policy.json
```

Если список уже содержит `DOMAIN\user`, параметр `-Domain` не нужен.

## Рекомендуемый phased rollout (изолированный профиль)

Для безопасного параллельного запуска рядом с legacy-инсталляцией используйте отдельные пути:

```powershell
.\windows\deploy-domain-users.ps1 `
  -ServerHost 10.10.10.13 `
  -ServerPort 5600 `
  -Domain SHARKON2025 `
  -Users user2,user3,user4,user5 `
  -InstallRoot 'C:\Program Files\AWatch-rus\bin' `
  -StateRoot 'C:\ProgramData\AWatch-rus' `
  -CustomRulesPath C:\Program Files\AWatch-rus\windows\web-category-rules.example.json `
  -CustomPolicyPath C:\Program Files\AWatch-rus\windows\dlp-policy.example.json
```

Single-user pilot в таком же стиле:

```powershell
.\windows\deploy-single-user.ps1 `
  -ServerHost 10.10.10.13 `
  -ServerPort 5600 `
  -TargetUser 'SHARKON2025\user1' `
  -InstallRoot 'C:\Program Files\AWatch-rus\bin' `
  -StateRoot 'C:\ProgramData\AWatch-rus' `
  -CustomRulesPath C:\Program Files\AWatch-rus\windows\web-category-rules.example.json `
  -CustomPolicyPath C:\Program Files\AWatch-rus\windows\dlp-policy.example.json
```

## Ensemble deploy (production workflow)

```powershell
.\windows\deploy-ensemble.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -Domain CONTOSO `
  -Users user1,user2,user3,user4,user5 `
  -CustomPolicyPath C:\Temp\dlp-policy.json `
  -ValidateAfterDeploy
```

Итоговый отчёт:

- `C:\ProgramData\AWatch-rus\ensemble-report-YYYYMMDD-HHMMSS.json`

## Категоризация доменов

- Встроенные категории покрывают базовые рабочие, нейтральные и личные домены.
- Для кастомизации скопируйте `windows/web-category-rules.example.json` и отредактируйте домены.
- Передайте файл через `-CustomRulesPath`; он будет сохранён как `C:\ProgramData\AWatch-rus\web-category-rules.json`.
- Пользовательские правила имеют приоритет над встроенными.

## Структура после установки

- `C:\Program Files\AWatch-rus\bin` — бинарники watcher'ов.
- `C:\ProgramData\AWatch-rus\deployment-config.json` — итоговая конфигурация.
- `C:\ProgramData\AWatch-rus\incident-artifacts\` — скриншоты DLP-инцидентов (если `incidentCapture.screenshotEnabled=true`).
- `C:\ProgramData\AWatch-rus\launch-watchers.ps1` — per-user launcher.
- `C:\ProgramData\AWatch-rus\recovery-loop.ps1` — system recovery loop.
- `C:\ProgramData\AWatch-rus\browser-domains-native-collector.ps1` — runtime collector.
- `C:\ProgramData\AWatch-rus\dlp-endpoint-signals-collector.ps1` — runtime endpoint collector.
- `C:\ProgramData\AWatch-rus\dlp-policy.json` — активная DLP-политика.
- `C:\ProgramData\AWatch-rus\logs\` — логи collector'а.

Для phased rollout те же файлы формируются в каталоге `StateRoot`, переданном параметром.

## Повторный прогон

- Скрипты идемпотентны: переустанавливают задачи и обновляют runtime-файлы.
- Предыдущая установка ActivityWatch бэкапится в `C:\ProgramData\AWatch-rus\backups\install-YYYYMMDD-HHMMSS`.
- Для жёсткого восстановления запускайте `windows/hardening-recovery.ps1`.
