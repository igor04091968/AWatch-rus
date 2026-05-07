# Компоненты системы

## Windows Collectors

### DLP Endpoint Collector
Мониторинг конечных точек для обнаружения утечек данных.

**Функции:**
- Clipboard мониторинг
- Print job мониторинг  
- USB write блокировка
- DLP правило evaluation
- Скриншоты при инцидентах

**Файл:** `windows/dlp-endpoint-signals-collector.ps1`

### Browser Domains Collector
Отслеживание посещаемых доменов с категоризацией.

**Функции:**
- Определение домена из URL
- Категоризация сайтов
- DLP проверка доменов
- Скриншоты при нарушениях

**Файл:** `windows/browser-domains-native-collector.ps1`

### Email Outbound Collector
Мониторинг исходящей почты.

**Функции:**
- Outlook Sent Items мониторинг
- SMTP соединения
- Email правило evaluation
- Блокировка нарушений

**Файл:** `windows/email-outbound-collector.ps1`

### Worktime Session Collector
Учет рабочего времени.

**Функции:**
- Определение сессий
- Учет перерывов
- Агрегация по дням

**Файл:** `windows/worktime-session-collector.ps1`

## Server Components

### ActivityWatch Server
Основной сервер для приема и хранения событий.

**Технологии:** Rust
**API:** HTTP на порту 5600

### PostgreSQL Database
Основное хранилище данных.

**Таблицы:**
- `dlp_events` - DLP инциденты
- `aggregation_state` - состояние агрегации
- `dlp_statistics` - статистика

### WebUI with RU Patches
Веб-интерфейс с русской локализацией.

**Файлы:** `aw-server/aw-ru-patch.js`, `aw-server/aw-sw-cleanup.js`

## Integration Components

### pfSense Poller
Сбор логов с pfSense firewall.

**Файл:** `pfsense/pfsense-aw-poller.py`

### DLP Aggregation Scripts
Агрегация DLP событий в PostgreSQL.

**Файл:** `scripts/aggregate_dlp_events.py`

### Prometheus Exporter
Экспорт метрик в формате Prometheus.

**Файл:** `grafana-1c/sql-exporter/collectors/aw_activitywatch.py`

## Monitoring Components

### Prometheus
Сбор и хранение метрик.

**Порт:** 9090

### Grafana
Визуализация метрик и дашборды.

**Порт:** 3000

### SQL Exporter
Прямые SQL запросы к PostgreSQL.

**Порт:** 9398

## Подробнее

- [DLP Endpoint Monitoring](DLP-Endpoint-Monitoring) - детально о DLP коллекторе
- [Browser Domains Monitoring](Browser-Domains-Monitoring) - детально о браузерном коллекторе
- [WebUI Русификация](WebUI-Russian-Patches) - детально о патчах интерфейса
