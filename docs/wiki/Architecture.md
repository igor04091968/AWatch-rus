# Архитектура системы

## Обзор

ActivityWatch-Russian состоит из нескольких слоев:

```
Windows Clients (сбор данных)
    ↓ HTTP API
Linux Server (хранение и обработка)
    ↓ TCP
Integration Layer (интеграции)
    ↓ HTTP/TCP
Monitoring Stack (визуализация)
```

## Слои архитектуры

### 1. Windows Clients - Сбор данных
- **DLP Endpoint Collector** - мониторинг clipboard, печати, USB
- **Browser Domains Collector** - мониторинг браузеров
- **Email Outbound Collector** - мониторинг почты
- **Worktime Session Collector** - учет рабочего времени

### 2. Linux Server - Хранение и обработка
- **ActivityWatch Server** - основной сервер (Rust)
- **PostgreSQL Database** - хранилище данных
- **WebUI with RU Patches** - веб-интерфейс с русификацией

### 3. Integration Layer - Интеграции
- **pfSense Poller** - сбор логов firewall
- **DLP Aggregation Scripts** - агрегация DLP событий
- **Prometheus Exporter** - экспорт метрик

### 4. Monitoring Stack - Мониторинг
- **Prometheus** - сбор метрик
- **Grafana** - визуализация
- **SQL Exporter** - прямые SQL запросы

## Потоки данных

### DLP инцидент
```
User → DLP Collector → Проверка правил → AW Server → PostgreSQL → Grafana
```

### Браузер мониторинг
```
Browser → Browser Collector → Категоризация → AW Server → WebUI
```

### Метрики
```
AW Server → Exporter → Prometheus → Grafana
```

## Порты

| Компонент | Порт | Протокол |
|-----------|------|----------|
| ActivityWatch API | 5600 | HTTP |
| ActivityWatch WebSocket | 5666 | WebSocket |
| PostgreSQL | 5432 | TCP |
| Prometheus | 9090 | HTTP |
| Grafana | 3000 | HTTP |
| Prometheus Exporter | 9398 | HTTP |

## Подробнее

- [Компоненты системы](Components) - детальное описание компонентов
- [Интерактивная карта](Interactive-Map) - визуальная схема связей
