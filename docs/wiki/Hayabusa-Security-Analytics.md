# Hayabusa Security Analytics

Эта страница описывает текущий production-контур Hayabusa внутри `AW-rus`.

## Что уже работает

- Windows-хост раз в `6` часов делает `EVTX export + upload`
- `AW-server` автоматически подхватывает пакет из `drop`
- `aw-hayabusa` строит forensic-отчёт
- `aw-hayabusa-case-alert` считает severity и score
- при уровне от `medium` создаётся или обновляется case
- при уровне от `high` уходит Telegram alert

## Операторский сценарий

На Windows-хосте:

```powershell
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-upload-hayabusa-to-aw-server.ps1 -HoursBack 6 -CaseId 30
```

На сервере для проверки:

```bash
cat /opt/hayabusa/state/latest-intake.json
journalctl -u aw-hayabusa-drop.service -n 80 --no-pager
curl -fsS http://127.0.0.1:5602/api/0/dlp/cases/30
```

## Что получает оператор

- `summary.html`
- `manifest.json`
- `run.log`
- `timeline.jsonl`
- `logon-summary-successful.csv`
- `logon-summary-failed.csv`
- case в DLP case API
- Telegram alert в операторский чат

## Как оценивается severity

Используются:

- Hayabusa `Level`
- top `RuleTitle`
- failed logons
- suspicious PowerShell
- credential-related detections
- timestomp detections

Выход:

- `low`
- `medium`
- `high`
- `critical`

## Границы

В case возвращается только bounded metadata:

- `tool`
- `host`
- `mode`
- `status`
- `intake_id`
- `package_path`
- `sha256`
- `report_dir`
- `summary_html`
- `timeline_path`
- `manifest_path`

Не возвращаются:

- сырые EVTX
- полный timeline body
- полный Sigma output

## Где настраивается

Windows:

- `aw_windows_hayabusa_auto_upload_enabled`
- `aw_windows_hayabusa_auto_upload_interval_hours`
- `aw_windows_hayabusa_auto_upload_hours_back`
- `aw_windows_hayabusa_auto_upload_mode`

Server:

- `aw_hayabusa_auto_case_enabled`
- `aw_hayabusa_auto_case_min_severity`
- `aw_hayabusa_telegram_enabled`
- `aw_hayabusa_telegram_min_severity`
- `aw_hayabusa_telegram_bot_token`
- `aw_hayabusa_telegram_chat_ids`

## Связанные документы

- [Runbook](../runbook.md)
- [Windows EVTX Export](../windows-hayabusa-evtx-export.md)
- [Security analytics stack v1](../security-analytics-stack-v1.md)
