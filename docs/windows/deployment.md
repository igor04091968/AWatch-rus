# Windows deployment

## Состав пакета

- `windows/deploy-single-user.ps1` — развёртывание для одного пользователя.
- `windows/deploy-domain-users.ps1` — массовое развёртывание по списку пользователей.
- `windows/deploy-ensemble.ps1` — orchestration-скрипт полного цикла (deploy + hardening + validation).
- `windows/hardening-recovery.ps1` — повторная регистрация задач, ACL и recovery-loop.
- `windows/validate-deployment.ps1` — машинная проверка состояния и JSON-отчёт.
- `windows/browser-domains-native-collector.ps1` — native collector доменов браузера с категоризацией.
- `windows/dlp-endpoint-signals-collector.ps1` — phase-2 collector (clipboard/USB/print signals).
- `windows/web-category-rules.example.json` — пример кастомных правил категоризации.
- `windows/dlp-policy.example.json` — пример DLP-политики (phase-1: alerting incidents).

## Что делает пакет

- Ставит `aw-watcher-afk` и `aw-watcher-window` из официального Windows ZIP ActivityWatch.
- Копирует browser-domain collector в `C:\ProgramData\ActivityWatch`.
- Копирует DLP policy в `C:\ProgramData\ActivityWatch\dlp-policy.json`.
- Включает `incidentCapture` в `deployment-config.json` для DLP-инцидентов:
  - `incidentCapture.screenshotEnabled = true`
  - `incidentCapture.artifactsRoot = <StateRoot>\incident-artifacts`
- Создаёт per-user задачи `ActivityWatch Launch [...]` с запуском при логоне.
- Создаёт системную задачу `ActivityWatch Recovery`, которая циклически перезапускает per-user launch tasks.
- Применяет ACL к `C:\Program Files\ActivityWatch`, `C:\ProgramData\ActivityWatch` и каталогу логов.
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
  -InstallRoot 'C:\Program Files\ActivityWatch-Phase2-u2u5' `
  -StateRoot 'C:\ProgramData\ActivityWatch-Phase2-u2u5' `
  -CustomRulesPath C:\Deploy\AWatch-rus\windows\web-category-rules.example.json `
  -CustomPolicyPath C:\Deploy\AWatch-rus\windows\dlp-policy.example.json
```

Single-user pilot в таком же стиле:

```powershell
.\windows\deploy-single-user.ps1 `
  -ServerHost 10.10.10.13 `
  -ServerPort 5600 `
  -TargetUser 'SHARKON2025\user1' `
  -InstallRoot 'C:\Program Files\ActivityWatch-Phase2' `
  -StateRoot 'C:\ProgramData\ActivityWatch-Phase2-user1' `
  -CustomRulesPath C:\Deploy\AWatch-rus\windows\web-category-rules.example.json `
  -CustomPolicyPath C:\Deploy\AWatch-rus\windows\dlp-policy.example.json
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

- `C:\ProgramData\ActivityWatch\ensemble-report-YYYYMMDD-HHMMSS.json`

## Категоризация доменов

- Встроенные категории покрывают базовые рабочие, нейтральные и личные домены.
- Для кастомизации скопируйте `windows/web-category-rules.example.json` и отредактируйте домены.
- Передайте файл через `-CustomRulesPath`; он будет сохранён как `C:\ProgramData\ActivityWatch\web-category-rules.json`.
- Пользовательские правила имеют приоритет над встроенными.

## Структура после установки

- `C:\Program Files\ActivityWatch` — бинарники watcher'ов.
- `C:\ProgramData\ActivityWatch\deployment-config.json` — итоговая конфигурация.
- `C:\ProgramData\ActivityWatch\incident-artifacts\` — скриншоты DLP-инцидентов (если `incidentCapture.screenshotEnabled=true`).
- `C:\ProgramData\ActivityWatch\launch-watchers.ps1` — per-user launcher.
- `C:\ProgramData\ActivityWatch\recovery-loop.ps1` — system recovery loop.
- `C:\ProgramData\ActivityWatch\browser-domains-native-collector.ps1` — runtime collector.
- `C:\ProgramData\ActivityWatch\dlp-endpoint-signals-collector.ps1` — runtime endpoint collector.
- `C:\ProgramData\ActivityWatch\dlp-policy.json` — активная DLP-политика.
- `C:\ProgramData\ActivityWatch\logs\` — логи collector'а.

Для phased rollout те же файлы формируются в каталоге `StateRoot`, переданном параметром.

## Повторный прогон

- Скрипты идемпотентны: переустанавливают задачи и обновляют runtime-файлы.
- Предыдущая установка ActivityWatch бэкапится в `C:\ProgramData\ActivityWatch\backups\install-YYYYMMDD-HHMMSS`.
- Для жёсткого восстановления запускайте `windows/hardening-recovery.ps1`.
