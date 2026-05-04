# Интерактивная карта архитектуры

Для визуального исследования связей между компонентами используйте интерактивную HTML карту.

## Как использовать

### Локально
```bash
# Открыть в браузере
xdg-open docs/diagrams/architecture-map.html

# Или
file:///path/to/AWatch-rus/docs/diagrams/architecture-map.html
```

### С GitHub
1. Скачайте файл: [architecture-map.html](https://github.com/igor04091968/AWatch-rus/blob/main/docs/diagrams/architecture-map.html)
2. Откройте в браузере

## Функции карты

### Визуальные компоненты
- **Windows Clients** (красный) - коллекторы данных
- **Linux Server** (бирюзовый) - сервер и БД
- **Integration** (желтый) - интеграции
- **Monitoring** (зеленый) - мониторинг стек
- **External** (фиолетовый) - внешние системы

### Интерактивность
- **Клик по компоненту** - показывает детальную информацию
- **Подсветка связей** - выделяет связанные компоненты
- **Поиск** - быстрый поиск по названию
- **Потоки данных** - визуализация процесса

### Информация о компоненте
При клике на компонент показывается:
- Описание
- Связи с другими компонентами
- Потоки данных
- Порты и протоколы

## Слои архитектуры

### 1. Windows Clients
- DLP Endpoint Collector
- Browser Domains Collector
- Email Outbound Collector
- Worktime Session Collector

### 2. Linux Server
- ActivityWatch Server
- PostgreSQL Database
- WebUI with RU Patches

### 3. Integration Layer
- pfSense Poller
- DLP Aggregation Scripts
- Prometheus Exporter

### 4. Monitoring Stack
- Prometheus
- Grafana
- SQL Exporter

### 5. External Systems
- pfSense Firewall
- Domain Controller

## Альтернативные визуализации

### ASCII схема
Для быстрого понимания: [architecture-simple.md](https://github.com/igor04091968/AWatch-rus/blob/main/docs/architecture-simple.md)

### Mermaid диаграммы
Для технических деталей: [architecture-diagram.md](https://github.com/igor04091968/AWatch-rus/blob/main/docs/architecture-diagram.md)

### Graphify knowledge graph
Для анализа кода: [graphify-out/index.html](https://github.com/igor04091968/AWatch-rus/blob/main/graphify-out/index.html)
