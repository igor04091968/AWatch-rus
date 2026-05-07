# DLP Endpoint Monitoring

Мониторинг конечных точек для обнаружения утечек данных через clipboard, печать и USB.

## Обзор

DLP Endpoint Collector отслеживает:
- **Clipboard** - буфер обмена
- **Print** - задания на печать
- **USB** - запись на USB накопители

## Архитектура

```
User Activity → DLP Collector → Rule Evaluation → Enforcement → ActivityWatch
```

## Функции

### Clipboard Monitoring
- Перехват clipboard событий
- Проверка по паттернам (credit cards, passport data, etc.)
- Блокировка копирования
- Скриншот при нарушении

### Print Monitoring
- Перехват print job событий
- Проверка принтеров и документов
- Блокировка печати
- Логирование попыток

### USB Monitoring
- Обнаружение USB устройств
- Блокировка записи
- Логирование подключений
- Политики по device ID

## Конфигурация

### DLP Policy
```json
{
  "clipboard_rules": [
    {
      "pattern": "\\b\\d{4}-\\d{4}-\\d{4}-\\d{4}\\b",
      "description": "Credit card numbers",
      "severity": "high",
      "action": "block"
    }
  ],
  "print_rules": [
    {
      "printer_match": "*",
      "document_keywords": ["confidential", "secret"],
      "action": "block"
    }
  ],
  "usb_rules": [
    {
      "device_id": "*",
      "action": "block_write"
    }
  ]
}
```

### Deployment Config
```json
{
  "aw_server_url": "http://aw-server:5600",
  "bucket_prefix": "aw-watcher-dlp-endpoint",
  "heartbeat_interval": 60,
  "screenshot_on_incident": true,
  "enforcement_enabled": true
}
```

## Установка

```powershell
# Копирование коллектора
Copy-Item windows/dlp-endpoint-signals-collector.ps1 C:\ProgramData\AWatch-rus\

# Настройка scheduled task
Register-ScheduledTask -TaskName "DLP Endpoint Collector" -Trigger $trigger -Action $action
```

## События

### DLP Incident Event
```json
{
  "timestamp": "2024-01-01T12:00:00Z",
  "type": "dlp_incident",
  "source": "clipboard",
  "rule_id": "credit_card_pattern",
  "severity": "high",
  "data": {
    "matched_text": "****-****-****-1234",
    "user": "user1",
    "host": "WORKSTATION01",
    "screenshot": "path/to/screenshot.png"
  }
}
```

## Требования

- Windows 10/11
- PowerShell 5.1+
- ActivityWatch installed
- Административные права (для enforcement)

## Мониторинг

- Количество инцидентов по типам
- Частота срабатываний правил
- Успешность enforcement действий
- Heartbeat статус

## Подробнее

- [Детальная диаграмма](https://github.com/igor04091968/AWatch-rus/blob/main/docs/diagrams/dlp-endpoint-monitoring.md)
- [DLP Правила](DLP-Rules)
