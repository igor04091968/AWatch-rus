# Email Outbound Collector

## Обзор

Мониторинг исходящей почты на Windows-эндпоинтах. Два режима работы:

| Режим    | Источник                       | Данные                                                |
|----------|--------------------------------|-------------------------------------------------------|
| outlook  | Outlook COM (Sent Items)       | Subject, From, To/CC, вложения, размер тела           |
| smtp     | `Get-NetTCPConnection`         | SMTP-соединения (порты 25/587/465/2525), процесс      |

По умолчанию `Mode = 'both'` — оба режима активны одновременно.

## Запуск

```powershell
# С deployment-config.json (штатный вариант)
.\email-outbound-collector.ps1

# С явными параметрами
.\email-outbound-collector.ps1 -ServerHost 10.10.10.13 -ServerPort 5600 -Mode outlook

# Только SMTP мониторинг (без Outlook)
.\email-outbound-collector.ps1 -ServerHost 10.10.10.13 -Mode smtp
```

### Параметры

| Параметр       | По умолчанию                            | Описание                        |
|----------------|-----------------------------------------|---------------------------------|
| `-ConfigPath`  | `C:\ProgramData\AWatch-rus\deployment-config.json` | Путь к конфигу            |
| `-ServerHost`  | из конфига                              | Адрес AW-сервера                |
| `-ServerPort`  | из конфига / 5600                       | Порт AW-сервера                 |
| `-PolicyPath`  | из конфига / `dlp-policy.json`          | Путь к DLP-политике             |
| `-Mode`        | `both`                                  | `outlook`, `smtp`, или `both`   |
| `-PollSeconds` | из конфига / 10                         | Интервал опроса                 |

## AW Buckets

- `aw-email-monitor_<host>` — все email-события (signal heartbeats)
- `aw-dlp-incidents_<host>` — инциденты при срабатывании DLP-правил

## DLP-политика: секция `endpoint.email`

Добавляется в существующий `dlp-policy.json`:

```json
{
  "endpoint": {
    "email": [
      {
        "id": "block-external-attachments",
        "action": "block",
        "severity": "high",
        "minAttachments": 1,
        "externalOnly": true,
        "internalDomain": "@company.ru",
        "message": "Запрещена отправка вложений на внешние адреса"
      },
      {
        "id": "alert-confidential-subject",
        "action": "alert",
        "severity": "medium",
        "subjectRegex": "(?i)(конфиденциально|секретно|для служебного пользования)",
        "message": "Обнаружена отправка письма с пометкой конфиденциальности"
      },
      {
        "id": "alert-personal-data",
        "action": "alert",
        "severity": "high",
        "recipientRegex": "(?i)(gmail\\.com|mail\\.ru|yandex\\.ru|yahoo\\.com)",
        "minAttachments": 1,
        "message": "Отправка вложений на личную почту"
      }
    ]
  }
}
```

### Параметры правил

| Поле              | Тип    | Описание                                                  |
|-------------------|--------|-----------------------------------------------------------|
| `id`              | string | Уникальный ID правила (обязательно)                       |
| `action`          | string | `alert` (по умолчанию) или `block`                        |
| `severity`        | string | `low`, `medium`, `high`, `critical`                       |
| `subjectRegex`    | string | Regex по теме письма                                      |
| `recipientRegex`  | string | Regex по списку получателей                               |
| `senderRegex`     | string | Regex по адресу отправителя                               |
| `attachmentRegex` | string | Regex по именам вложений                                  |
| `minAttachments`  | int    | Минимальное количество вложений для срабатывания           |
| `minBodyLength`   | int    | Минимальная длина тела письма                             |
| `externalOnly`    | bool   | Срабатывать только на внешних получателей                  |
| `internalDomain`  | string | Домен организации (используется с `externalOnly`)         |
| `cooldownSeconds` | int    | Cooldown между повторными инцидентами                     |
| `message`         | string | Текст уведомления пользователю и в инцидент               |

## Enforcement (action: "block")

**Outlook mode**: письмо перемещается из Sent Items в Drafts. Пользователь получает balloon notification.

**SMTP mode**: только уведомление (перехват SMTP-соединения на сетевом уровне не реализуем из PowerShell). Инцидент записывается с `enforced: false`.

## Телеметрия

### Heartbeat `email_sent` (Outlook mode)
```json
{
  "signalType": "email_sent",
  "subject": "<sha256 hash>",
  "sender": "user@company.ru",
  "recipientCount": 3,
  "recipients": "<sha256 hash>",
  "attachmentCount": 2,
  "attachmentNames": "report.xlsx; data.csv",
  "bodyLength": 1520,
  "collectionMode": "outlook"
}
```

### Heartbeat `smtp_connection` (SMTP mode)
```json
{
  "signalType": "smtp_connection",
  "remoteAddress": "74.125.205.108",
  "remotePort": 587,
  "processId": 12340,
  "processName": "OUTLOOK",
  "collectionMode": "smtp"
}
```

### Incident
```json
{
  "ruleId": "block-external-attachments",
  "action": "block",
  "severity": "high",
  "signalType": "email_outbound",
  "subject": "<sha256>",
  "attachmentCount": 2,
  "enforced": true
}
```

## Приватность

- Тема и получатели записываются как SHA256-хеш (не открытый текст).
- Тело письма не читается и не хранится — записывается только длина.
- Имена вложений записываются открытым текстом (для DLP-анализа).

## Интеграция в ensemble

Добавьте в `launch-watchers.ps1` или Task Scheduler:

```powershell
Start-Process powershell.exe -ArgumentList '-ExecutionPolicy Bypass -File "C:\ProgramData\AWatch-rus\email-outbound-collector.ps1"' -WindowStyle Hidden
```

## Требования

- **Outlook mode**: Microsoft Outlook установлен и настроен для текущего пользователя.
- **SMTP mode**: Не требует дополнительного ПО. Работает на уровне TCP-соединений.
- **Enforcement (block)**: Outlook mode — требует доступ к COM объекту Outlook.
