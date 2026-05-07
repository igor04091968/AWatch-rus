# DLP Events Aggregation - Компонентная диаграмма

## Обзор
Система агрегации DLP событий из ActivityWatch buckets в PostgreSQL для аналитики и отчетности.

## Архитектура

```mermaid
graph TB
    subgraph "Aggregation Pipeline"
        A[Main Orchestrator]
        B[Bucket Discovery]
        C[Event Fetcher]
        D[Event Parser]
        E[Normalizer]
        F[PostgreSQL Writer]
        G[State Manager]
    end

    subgraph "Data Sources"
        H[ActivityWatch API]
        I[SQLite Buckets]
        J[PostgreSQL Database]
    end

    subgraph "Output"
        K[Aggregated Events]
        L[Statistics]
        M[Reports]
    end

    A --> B
    A --> C
    A --> G
    B --> H
    C --> H
    C --> I
    D --> C
    E --> D
    F --> E
    F --> J
    G --> J
    
    F --> K
    F --> L
    F --> M
    
    style A fill:#4ecdc4
    style F fill:#ff6b6b
    style G fill:#ffe66d
```

## Потоки данных

### Aggregation Flow
```mermaid
sequenceDiagram
    participant Orchestrator as Main Orchestrator
    participant State as State Manager
    participant AW as ActivityWatch API
    participant Fetcher as Event Fetcher
    participant Parser as Event Parser
    participant PG as PostgreSQL

    Orchestrator->>State: Load last sync state
    State->>Orchestrator: Return last timestamp
    Orchestrator->>AW: List buckets
    AW->>Orchestrator: Return bucket list
    loop For each DLP bucket
        Orchestrator->>AW: Get events since last sync
        AW->>Fetcher: Return events
        Fetcher->>Parser: Parse event structure
        Parser->>Parser: Extract DLP data
        Parser->>Orchestrator: Return parsed events
        Orchestrator->>PG: Insert batch of events
        PG->>Orchestrator: Confirm insert
    end
    Orchestrator->>State: Save new sync state
    State->>PG: Persist state
```

### Event Processing Pipeline
```mermaid
graph LR
    A[Raw Event] --> B[Parse JSON]
    B --> C[Extract DLP Fields]
    C --> D[Normalize Timestamp]
    D --> E[Enrich with User/Host]
    E --> F[Validate Data]
    F --> G[Batch Prepare]
    G --> H[PostgreSQL Insert]
```

## Ключевые функции

### Оркестрация
- `aggregate_dlp_events_main()` - главная функция агрегации
- `aggregate_dlp_events_list_buckets()` - получение списка buckets
- `aggregate_dlp_events_get_start_time()` - определение начала периода

### Получение событий
- `aggregate_dlp_events_fetch_bucket_events()` - получение событий из bucket
- `aggregate_dlp_events_bucket_stream_type()` - определение типа потока
- `aggregate_dlp_events_aw_get_json()` - HTTP запрос к AW API

### Парсинг
- `aggregate_dlp_events_awevent()` - парсинг события AW
- `aggregate_dlp_events_parse_timestamp()` - парсинг timestamp
- `aggregate_dlp_events_parse_prefixes()` - парсинг префиксов bucket
- `aggregate_dlp_events_event_row()` - формирование строки события

### Нормализация
- `aggregate_dlp_events_normalize_base_url()` - нормализация URL AW API
- `aggregate_dlp_events_format_aw_timestamp()` - форматирование timestamp
- `aggregate_dlp_events_bool_as_int()` - конвертация boolean в int
- `aggregate_dlp_events_first_string()` - получение первой строки

### Батчинг
- `aggregate_dlp_events_bucket()` - обработка bucket
- `aggregate_dlp_events_build_events_path()` - построение пути к событиям
- `aggregate_dlp_events_build_parser()` - создание парсера

### Соединения
- `aggregate_dlp_events_connect_sqlite()` - соединение с SQLite
- `aggregate_dlp_events_psycopgconnection()` - соединение с PostgreSQL
- `aggregate_dlp_events_psycopgconnection_cursor()` - курсор PostgreSQL
- `aggregate_dlp_events_psycopgconnection_commit()` - коммит транзакции

### Запись
- `aggregate_dlp_events_insert_events()` - вставка событий
- `aggregate_dlp_events_insert_postgres_events()` - вставка в PostgreSQL
- `aggregate_dlp_events_select_buckets()` - выборка buckets
- `aggregate_dlp_events_ensure_schema()` - обеспечение схемы БД

### Управление состоянием
- `aggregate_dlp_events_load_state()` - загрузка состояния
- `aggregate_dlp_events_save_state()` - сохранение состояния
- `aggregate_dlp_events_event_key()` - ключ события

### Утилиты
- `aggregate_dlp_events_utc_now()` - текущее UTC время
- `aggregate_dlp_events_ensure_postgres_schema()` - схема PostgreSQL

## Схема базы данных

### Таблица dlp_events
```sql
CREATE TABLE dlp_events (
    id SERIAL PRIMARY KEY,
    timestamp TIMESTAMP NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    source VARCHAR(50) NOT NULL,
    rule_id VARCHAR(100),
    severity VARCHAR(20),
    user_id VARCHAR(100),
    host_id VARCHAR(100),
    data JSONB,
    created_at TIMESTAMP DEFAULT NOW(),
    INDEX idx_timestamp (timestamp),
    INDEX idx_user (user_id),
    INDEX idx_host (host_id),
    INDEX idx_severity (severity),
    INDEX idx_type (event_type)
);
```

### Таблица aggregation_state
```sql
CREATE TABLE aggregation_state (
    id SERIAL PRIMARY KEY,
    bucket_id VARCHAR(255) UNIQUE NOT NULL,
    last_synced_timestamp TIMESTAMP,
    last_synced_at TIMESTAMP DEFAULT NOW(),
    events_processed INTEGER DEFAULT 0
);
```

### Таблица dlp_statistics
```sql
CREATE TABLE dlp_statistics (
    id SERIAL PRIMARY KEY,
    date DATE NOT NULL,
    user_id VARCHAR(100),
    host_id VARCHAR(100),
    event_type VARCHAR(50),
    severity VARCHAR(20),
    incident_count INTEGER DEFAULT 0,
    UNIQUE(date, user_id, host_id, event_type, severity)
);
```

## Конфигурация

### Config File
```json
{
  "aw_base_url": "http://aw-server:5600",
  "postgres_url": "postgresql://user:pass@localhost:5432/activitywatch",
  "batch_size": 1000,
  "sync_interval_minutes": 5,
  "bucket_prefixes": [
    "aw-watcher-dlp-endpoint",
    "aw-watcher-browser-domains",
    "aw-watcher-email-outbound"
  ],
  "retention_days": 90
}
```

### Environment Variables
```bash
AW_BASE_URL=http://aw-server:5600
POSTGRES_URL=postgresql://aw:password@localhost:5432/activitywatch
BATCH_SIZE=1000
SYNC_INTERVAL=300
LOG_LEVEL=INFO
```

## События

### Input Event (from ActivityWatch)
```json
{
  "id": "event_id",
  "timestamp": "2024-01-01T12:00:00Z",
  "duration": 60.0,
  "data": {
    "type": "dlp_incident",
    "source": "clipboard",
    "rule_id": "credit_card_pattern",
    "severity": "high",
    "user": "user1",
    "host": "WORKSTATION01",
    "matched_text": "****-****-****-1234"
  }
}
```

### Output Event (in PostgreSQL)
```sql
INSERT INTO dlp_events (
    timestamp, event_type, source, rule_id, 
    severity, user_id, host_id, data
) VALUES (
    '2024-01-01 12:00:00',
    'dlp_incident',
    'clipboard',
    'credit_card_pattern',
    'high',
    'user1',
    'WORKSTATION01',
    '{"matched_text": "****-****-****-1234"}'::jsonb
);
```

## Производительность

### Оптимизации
- Batch вставки (по 1000 событий)
- Connection pooling к PostgreSQL
- Асинхронная обработка
- Индексы на частых запросах

### Метрики производительности
```python
performance_metrics = {
    "events_per_second": 100,
    "batch_insert_time_ms": 50,
    "api_latency_ms": 20,
    "postgres_write_latency_ms": 30
}
```

### Мониторинг
- Время обработки batch
- Размер очереди событий
- Ошибки соединения с PostgreSQL
- Latency API запросов

## Обработка ошибок

### Retry Strategy
```python
retry_config = {
    "max_retries": 3,
    "backoff_seconds": [1, 5, 15],
    "retry_on": [
        "ConnectionError",
        "TimeoutError",
        "DatabaseError"
    ]
}
```

### Dead Letter Queue
```sql
CREATE TABLE dlp_events_failed (
    id SERIAL PRIMARY KEY,
    raw_event JSONB,
    error_message TEXT,
    failed_at TIMESTAMP DEFAULT NOW(),
    retry_count INTEGER DEFAULT 0
);
```

## Планирование

### Cron Job
```cron
*/5 * * * * /usr/bin/python3 /path/to/aggregate_dlp_events.py
```

### Systemd Service
```ini
[Unit]
Description=ActivityWatch DLP Aggregation
After=network.target

[Service]
Type=simple
User=aw-aggregator
ExecStart=/usr/bin/python3 /path/to/aggregate_dlp_events.py
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

## Отчеты

### Daily Report
```sql
SELECT 
    date,
    event_type,
    severity,
    COUNT(*) as incident_count
FROM dlp_events
WHERE date >= CURRENT_DATE - INTERVAL '7 days'
GROUP BY date, event_type, severity
ORDER BY date DESC, incident_count DESC;
```

### User Summary
```sql
SELECT 
    user_id,
    COUNT(*) as total_incidents,
    COUNT(CASE WHEN severity = 'high' THEN 1 END) as high_severity,
    COUNT(CASE WHEN severity = 'medium' THEN 1 END) as medium_severity
FROM dlp_events
WHERE timestamp >= CURRENT_DATE - INTERVAL '30 days'
GROUP BY user_id
ORDER BY total_incidents DESC;
```

## Валидация данных

### Checks
- Timestamp в допустимом диапазоне
- Обязательные поля заполнены
- Severity в списке допустимых значений
- User/Host существуют в справочниках

### Data Quality
```python
validation_rules = {
    "timestamp": "required, past_date",
    "event_type": "required, in_list",
    "severity": "required, in_list",
    "user_id": "required, max_length=100",
    "host_id": "required, max_length=100"
}
```
