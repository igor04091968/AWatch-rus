# DLP Enforcement (action: "block")

## Обзор

Phase 2.5 расширяет DLP endpoint collector функциями **активного предотвращения** (enforcement).
При `action: "block"` в правиле DLP-политики коллектор не только регистрирует инцидент, но и выполняет блокирующее действие:

| Канал     | Действие при `block`                                      |
|-----------|-----------------------------------------------------------|
| clipboard | Очистка буфера обмена (`Set-Clipboard -Value $null`)      |
| usb       | Перевод USB-диска в read-only (`Set-Disk -IsReadOnly`)    |
| print     | Отмена задания печати (`Remove-CimInstance Win32_PrintJob`)|

Во всех случаях пользователь получает Windows-уведомление (balloon notification) с описанием причины блокировки.

## Конфигурация политики

Формат `dlp-policy.json` не изменился — поле `action` в правиле теперь поддерживает значение `"block"` наряду с `"alert"` (по умолчанию).

### Пример: блокировка USB записи

```json
{
  "defaults": {
    "enabled": true,
    "action": "alert",
    "severity": "medium",
    "cooldownSeconds": 300
  },
  "endpoint": {
    "usb": [
      {
        "id": "block-all-usb-write",
        "action": "block",
        "severity": "high",
        "message": "Запись на USB-носитель заблокирована политикой DLP"
      }
    ],
    "clipboard": [
      {
        "id": "block-pdn-clipboard",
        "action": "block",
        "severity": "high",
        "regexPatterns": [
          "\\b\\d{3}-\\d{3}-\\d{3}\\s?\\d{2}\\b",
          "\\b\\d{4}\\s?\\d{6}\\b"
        ],
        "minLength": 8,
        "message": "Буфер обмена очищен: обнаружены персональные данные (СНИЛС/паспорт)"
      }
    ],
    "print": [
      {
        "id": "block-confidential-print",
        "action": "block",
        "severity": "high",
        "documentRegex": "(?i)(конфиденциально|секретно|confidential|restricted)",
        "message": "Печать заблокирована: документ содержит метку конфиденциальности"
      }
    ]
  }
}
```

### Пример: только мониторинг (без блокировки)

```json
{
  "endpoint": {
    "usb": [
      {
        "id": "monitor-usb",
        "action": "alert",
        "severity": "medium",
        "message": "Обнаружено подключение USB-носителя"
      }
    ]
  }
}
```

## Телеметрия

Каждый инцидент с enforcement записывается в bucket `aw-dlp-incidents_<host>` с дополнительным полем:

```json
{
  "ruleId": "block-all-usb-write",
  "action": "block",
  "severity": "high",
  "signalType": "usb_insert",
  "enforced": true,
  "driveLetter": "E:",
  "volumeName": "FLASH_DRIVE"
}
```

- `enforced: true` — блокировка выполнена успешно
- `enforced: false` — блокировка не удалась (недостаточно прав, устройство недоступно и т.д.)

## Требования

- **Clipboard block**: Не требует повышенных прав.
- **USB write-block**: Требует запуск от имени администратора (для `Set-Disk -IsReadOnly`). При запуске без прав блокировка не сработает, но инцидент будет зарегистрирован с `enforced: false`.
- **Print block**: Требует права на отмену заданий печати (обычно — SYSTEM или администратор принт-сервера).

## Уведомления

При каждой блокировке пользователю показывается Windows balloon notification:

| Канал     | Заголовок                            |
|-----------|--------------------------------------|
| clipboard | `DLP: буфер обмена очищен`           |
| usb       | `DLP: USB заблокирован для записи`   |
| print     | `DLP: печать заблокирована`          |

Текст уведомления берётся из поля `message` правила политики.

## Rollback

Для отключения enforcement без изменения кода — смените `action` с `"block"` на `"alert"` в `dlp-policy.json`. Все правила продолжат мониторинг без блокировки.

Для USB, переведённого в read-only, восстановление:
```powershell
Get-Disk | Where-Object { $_.BusType -eq 'USB' -and $_.IsReadOnly } | Set-Disk -IsReadOnly $false
```
