# Windows deployment ensemble (PowerShell)

Документ фиксирует профессиональный workflow развёртывания и контроля ActivityWatch-клиентов в домене Windows.

## Полные пути

- `<PROJECT_ROOT>/windows/deploy-ensemble.ps1`
- `<PROJECT_ROOT>/windows/deploy-domain-users.ps1`
- `<PROJECT_ROOT>/windows/deploy-single-user.ps1`
- `<PROJECT_ROOT>/windows/hardening-recovery.ps1`
- `<PROJECT_ROOT>/windows/validate-deployment.ps1`
- `<PROJECT_ROOT>/windows/ActivityWatch.Windows.Common.psm1`
- `<PROJECT_ROOT>/windows/ActivityWatch.Windows.Common.psd1`

## Рекомендованный запуск

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope Process

C:\Program Files\AWatch-rus\windows\deploy-ensemble.ps1 `
  -ServerHost <AW_SERVER_HOST> `
  -ServerPort 5600 `
  -Domain SHARKON2025 `
  -Users user1,user2,user3,user4,user5 `
  -InstallRoot 'C:\Program Files\AWatch-rus\bin' `
  -StateRoot 'C:\ProgramData\AWatch-rus' `
  -AfkEnabled:$false `
  -CustomPolicyPath C:\Program Files\AWatch-rus\windows\dlp-policy.example.json `
  -ValidateAfterDeploy
```

## Что делает `deploy-ensemble.ps1`

1. Нормализует список пользователей (`DOMAIN\user`).
2. Вызывает массовый деплой `deploy-domain-users.ps1`.
3. Применяет hardening/recovery (`hardening-recovery.ps1`), если не задан `-SkipHardening`.
4. Опционально запускает контроль (`validate-deployment.ps1`) при `-ValidateAfterDeploy`.
5. Пишет итоговый JSON-отчёт в `<StateRoot>\ensemble-report-*.json`.

Для quiet-профиля без `afkstatus` используйте `-AfkEnabled:$false`; если не нужен window watcher, добавьте `-WindowEnabled:$false`.

## Быстрый health-check

```powershell
$report = C:\Program Files\AWatch-rus\windows\validate-deployment.ps1 `
  -ConfigPath C:\ProgramData\AWatch-rus\deployment-config.json
$report | ConvertTo-Json -Depth 12
```

Ожидается:

- `overallOk = true`
- нет missing-файлов
- все `ActivityWatch*` Scheduled Task присутствуют
- активны процессы `aw-watcher-afk` и `aw-watcher-window`
