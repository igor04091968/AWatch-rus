# DLP Endpoint Monitoring - Компонентная диаграмма

## Обзор
Мониторинг конечных точек для обнаружения утечек данных через clipboard, печать и USB.

## Архитектура

```mermaid
graph TB
    subgraph "DLP Endpoint Collector"
        A[Main Loop]
        B[Load DLP Policy]
        C[Deployment Config]
        D[Clipboard Monitor]
        E[Print Monitor]
        F[USB Monitor]
        G[Rule Engine]
        H[Enforcement Actions]
        I[Event Logging]
        J[Screenshot Capture]
    end

    subgraph "External Dependencies"
        K[ActivityWatch API]
        L[DLP Policy JSON]
        M[Windows API]
        N[File System]
    end

    A --> B
    A --> C
    B --> G
    C --> A
    
    D --> G
    E --> G
    F --> G
    
    G --> H
    G --> I
    G --> J
    
    H --> M
    I --> N
    J --> N
    
    I --> K
    A --> K
    
    B --> L
    C --> L
    
    style D fill:#ff6b6b
    style E fill:#ff6b6b
    style F fill:#ff6b6b
    style G fill:#4ecdc4
    style H fill:#ffe66d
```

## Потоки данных

### Clipboard Monitoring Flow
```mermaid
sequenceDiagram
    participant User
    participant Clipboard as Clipboard API
    participant Collector as DLP Collector
    participant RuleEngine as Rule Engine
    participant AW as ActivityWatch
    participant FS as File System

    User->>Clipboard: Copy data
    Clipboard->>Collector: Clipboard change event
    Collector->>RuleEngine: Evaluate against rules
    RuleEngine->>RuleEngine: Check patterns
    alt Pattern Match
        RuleEngine->>Collector: Trigger incident
        Collector->>FS: Capture screenshot
        Collector->>Collector: Apply enforcement
        Collector->>AW: Send DLP incident
    else No Match
        RuleEngine->>Collector: Allow
    end
```

### Print Monitoring Flow
```mermaid
sequenceDiagram
    participant App as Application
    participant PrintSpooler as Print Spooler
    participant Collector as DLP Collector
    participant RuleEngine as Rule Engine
    participant AW as ActivityWatch

    App->>PrintSpooler: Print job
    PrintSpooler->>Collector: Print service event
    Collector->>Collector: Extract document info
    Collector->>RuleEngine: Evaluate printer rules
    alt Violation
        RuleEngine->>Collector: Block print job
        Collector->>PrintSpooler: Cancel job
        Collector->>AW: Log incident
    else Allowed
        Collector->>AW: Log print activity
    end
```

### USB Monitoring Flow
```mermaid
sequenceDiagram
    participant User
    participant USB as USB Device
    participant Windows as Windows API
    participant Collector as DLP Collector
    participant RuleEngine as Rule Engine
    participant AW as ActivityWatch

    User->>USB: Insert USB drive
    USB->>Windows: Device connect
    Windows->>Collector: USB device event
    Collector->>RuleEngine: Check USB rules
    alt Write Blocked
        RuleEngine->>Collector: Block write
        Collector->>Windows: Prevent write
        Collector->>AW: Log blocked attempt
    else Write Allowed
        Collector->>AW: Log file copy
    end
```

## Ключевые функции

### Основные функции
- `dlp_endpoint_signals_collector_main()` - главный цикл коллектора
- `dlp_endpoint_signals_collector_load_dlppolicy()` - загрузка DLP правил
- `dlp_endpoint_signals_collector_get_deploymentconfig()` - чтение конфигурации

### Мониторинг
- `dlp_endpoint_signals_collector_evaluate_clipboardrules()` - проверка clipboard
- `dlp_endpoint_signals_collector_evaluate_printrules()` - проверка печати
- `dlp_endpoint_signals_collector_evaluate_usbrules()` - проверка USB

### Принудительные действия
- `dlp_endpoint_signals_collector_invoke_clipboardenforcement()` - блокировка clipboard
- `dlp_endpoint_signals_collector_invoke_printjobenforcement()` - блокировка печати
- `dlp_endpoint_signals_collector_invoke_usbwriteblockenforcement()` - блокировка USB

### Логирование
- `dlp_endpoint_signals_collector_write_endpointlog()` - запись логов коллектора
- `dlp_endpoint_signals_collector_send_dlpincidentheartbeat()` - heartbeat инцидентов
- `dlp_endpoint_signals_collector_capture_incidentscreenshot()` - захват скриншота

## Конфигурация

### DLP Policy Structure
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
    "application": "chrome.exe",
    "screenshot": "path/to/screenshot.png"
  }
}
```

### Heartbeat Event
```json
{
  "timestamp": "2024-01-01T12:00:00Z",
  "type": "heartbeat",
  "status": "running",
  "incidents_count": 5,
  "last_incident": "2024-01-01T11:55:00Z"
}
```

## Зависимости

### Windows API
- Clipboard API
- Print Spooler API
- USB Device Notification API
- Process API

### External Services
- ActivityWatch HTTP API
- File system (для скриншотов и логов)

## Развертывание

### Требования
- Windows 10/11
- PowerShell 5.1+
- ActivityWatch installed
- Административские права (для enforcement)

### Установка
```powershell
# Копирование коллектора
Copy-Item dlp-endpoint-signals-collector.ps1 C:\ProgramData\AWatch-rus\

# Настройка scheduled task
Register-ScheduledTask -TaskName "DLP Endpoint Collector" -Trigger $trigger -Action $action
```

## Мониторинг

### Метрики
- Количество инцидентов по типам (clipboard/print/USB)
- Частота срабатываний правил
- Успешность enforcement действий
- Heartbeat статус

### Алерты
- Коллектор не отправляет heartbeat > 5 минут
- Высокая частота DLP инцидентов
- Ошибки enforcement действий
