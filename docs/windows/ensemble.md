# Windows deployment ensemble (PowerShell)

Документ фиксирует профессиональный workflow развёртывания и контроля ActivityWatch-клиентов в домене Windows.

## Полные пути

- `/home/igor/tmp/AWatch-rus/windows/deploy-ensemble.ps1`
- `/home/igor/tmp/AWatch-rus/windows/deploy-domain-users.ps1`
- `/home/igor/tmp/AWatch-rus/windows/deploy-single-user.ps1`
- `/home/igor/tmp/AWatch-rus/windows/hardening-recovery.ps1`
- `/home/igor/tmp/AWatch-rus/windows/validate-deployment.ps1`
- `/home/igor/tmp/AWatch-rus/windows/ActivityWatch.Windows.Common.psm1`
- `/home/igor/tmp/AWatch-rus/windows/ActivityWatch.Windows.Common.psd1`

## Рекомендованный запуск

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope Process

C:\Deploy\AWatch-rus\windows\deploy-ensemble.ps1 `
  -ServerHost 10.10.10.13 `
  -ServerPort 5600 `
  -Domain AD `
  -Users user1,user2,user3,user4,user5 `
  -CustomPolicyPath C:\Deploy\AWatch-rus\windows\dlp-policy.example.json `
  -ValidateAfterDeploy
```

## Что делает `deploy-ensemble.ps1`

1. Нормализует список пользователей (`DOMAIN\user`).
2. Вызывает массовый деплой `deploy-domain-users.ps1`.
3. Применяет hardening/recovery (`hardening-recovery.ps1`), если не задан `-SkipHardening`.
4. Опционально запускает контроль (`validate-deployment.ps1`) при `-ValidateAfterDeploy`.
5. Пишет итоговый JSON-отчёт в `C:\ProgramData\ActivityWatch\ensemble-report-*.json`.

## Быстрый health-check

```powershell
$report = C:\Deploy\AWatch-rus\windows\validate-deployment.ps1 `
  -ConfigPath C:\ProgramData\ActivityWatch\deployment-config.json
$report | ConvertTo-Json -Depth 12
```

Ожидается:

- `overallOk = true`
- нет missing-файлов
- все `ActivityWatch*` Scheduled Task присутствуют
- активны процессы `aw-watcher-afk` и `aw-watcher-window`
