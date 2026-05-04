# ActivityWatch-Russian - Простая архитектура

## Общая схема (сверху вниз)

```
┌─────────────────────────────────────────────────────────────────┐
│                        ПОЛЬЗОВАТЕЛИ                              │
│                    (Windows рабочие станции)                     │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           │ Коллекторы (PowerShell)
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│              Windows Collectors (на каждой машине)               │
├──────────────────────────────────────────────────────────────────┤
│  • DLP Endpoint Collector   - буфер, печать, USB                 │
│  • Browser Domains Collector - посещаемые сайты                  │
│  • Email Outbound Collector - исходящая почта                    │
│  • Worktime Session Collector - рабочее время                   │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           │ HTTP API (отправка событий)
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                  ActivityWatch Server (Linux)                    │
├──────────────────────────────────────────────────────────────────┤
│  • Принимает события от коллекторов                              │
│  • Хранит в базе данных                                          │
│  • Предоставляет WebUI и API                                     │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           │ Хранение данных
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                    PostgreSQL Database                           │
├──────────────────────────────────────────────────────────────────┤
│  • События активности                                            │
│  • DLP инциденты                                                 │
│  • Метаданные хостов и пользователей                             │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           │ Чтение/Обработка
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                 Обработка и Агрегация                            │
├──────────────────────────────────────────────────────────────────┤
│  • aggregate_dlp_events.py   - агрегация DLP событий             │
│  • pfSense Poller            - логи firewall                    │
│  • Prometheus Exporter       - метрики для мониторинга           │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           │ Визуализация
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                 Grafana + Prometheus Stack                       │
├──────────────────────────────────────────────────────────────────┤
│  • Prometheus  - сбор метрик                                     │
│  • Grafana     - дашборды и визуализация                         │
│  • SQL Exporter - прямой доступ к PostgreSQL                     │
└──────────────────────────────────────────────────────────────────┘
```

## Потоки данных (по направлениям)

### Поток 1: DLP мониторинг
```
Пользователь → DLP Collector → Проверка правил → AW Server → PostgreSQL → Grafana
```

### Поток 2: Мониторинг браузеров
```
Браузер → Browser Collector → Категоризация → AW Server → WebUI Dashboard
```

### Поток 3: Мониторинг почты
```
Outlook/SMTP → Email Collector → Проверка правил → AW Server → Grafana
```

### Поток 4: pfFirewall логи
```
pfSense → Poller (Python) → HTTP API → AW Server → Grafana
```

### Поток 5: Метрики
```
AW Server → Exporter (Python) → /metrics → Prometheus → Grafana
```

## Компоненты по уровням

### Уровень 1: Сбор данных (Windows)
```
┌─────────────────────────────────────┐
│  Windows Collectors (PowerShell)    │
│  ┌───────────────────────────────┐  │
│  │ • dlp-endpoint-signals-collector│  │
│  │ • browser-domains-collector    │  │
│  │ • email-outbound-collector     │  │
│  │ • worktime-session-collector   │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

### Уровень 2: Хранение и обработка (Linux Server)
```
┌─────────────────────────────────────┐
│  ActivityWatch Server + PostgreSQL  │
│  ┌───────────────────────────────┐  │
│  │ • aw-server (Rust)            │  │
│  │ • PostgreSQL Database        │  │
│  │ • WebUI (с RU патчами)        │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

### Уровень 3: Интеграции и обработка
```
┌─────────────────────────────────────┐
│  Интеграции и Скрипты (Python)       │
│  ┌───────────────────────────────┐  │
│  │ • aggregate_dlp_events.py    │  │
│  │ • pfsense-aw-poller.py        │  │
│  │ • aw_activitywatch_exporter   │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

### Уровень 4: Визуализация и мониторинг
```
┌─────────────────────────────────────┐
│  Grafana + Prometheus (Docker)       │
│  ┌───────────────────────────────┐  │
│  │ • Prometheus (порт 9090)       │  │
│  │ • Grafana (порт 3000)          │  │
│  │ • SQL Exporter (порт 9398)    │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

## Развертывание

### На Windows рабочих станциях
```
1. Установка ActivityWatch (через InnoSetup installer)
2. Развертывание коллекторов (deploy-domain-users.ps1)
3. Настройка scheduled tasks
4. Конфигурация DLP политик
```

### На Linux сервере
```
1. Установка ActivityWatch Server
2. Настройка PostgreSQL
3. Применение RU патчей к WebUI
4. Запуск скриптов агрегации
5. Запуск Prometheus Exporter
```

### Мониторинг стек
```
1. Docker Compose развертывание
2. Настройка Prometheus scrape config
3. Импорт Grafana дашбордов
4. Настройка алертов
```

## Ключевые файлы

```
ActivityWatch-Russian/
├── windows/                    # Windows коллекторы
│   ├── dlp-endpoint-signals-collector.ps1
│   ├── browser-domains-native-collector.ps1
│   ├── email-outbound-collector.ps1
│   └── deploy-domain-users.ps1
│
├── scripts/                    # Серверные скрипты
│   └── aggregate_dlp_events.py
│
├── aw-server/                  # Патчи WebUI
│   └── aw-ru-patch.js
│
├── pfsense/                    # Интеграция pfSense
│   └── pfsense-aw-poller.py
│
└── grafana-1c/                 # Мониторинг стек
    ├── docker-compose.yml
    ├── prometheus/prometheus.yml
    └── grafana/dashboards/
```

## Порты

| Компонент | Порт | Протокол |
|-----------|------|----------|
| ActivityWatch WebUI | 5600 | HTTP |
| ActivityWatch API | 5666 | WebSocket |
| Prometheus | 9090 | HTTP |
| Prometheus Exporter | 9398 | HTTP |
| Grafana | 3000 | HTTP |
| PostgreSQL | 5432 | TCP |

## Быстрый старт

### Запуск мониторинг стека
```bash
cd grafana-1c
docker-compose up -d
```

### Развертывание на Windows
```powersShell
.\windows\deploy-domain-users.ps1
```

### Агрегация DLP событий
```bash
python3 scripts/aggregate_dlp_events.py
```

## Связи между компонентами

```
Windows Collectors
    │
    ├─► ActivityWatch Server (HTTP API)
    │       │
    │       ├─► PostgreSQL (хранение)
    │       │
    │       ├─► WebUI (RU патчи)
    │       │
    │       └─► Prometheus Exporter (метрики)
    │               │
    │               └─► Prometheus
    │                       │
    │                       └─► Grafana
    │
pfSense Firewall
    │
    └─► pfSense Poller
            │
            └─► ActivityWatch Server
                    │
                    └─► PostgreSQL
```

## Основные сценарии

### Сценарий 1: Пользователь копирует конфиденциальные данные
```
1. Пользователь копирует текст в буфер обмена
2. DLP Collector перехватывает событие
3. Проверка по DLP правилам
4. При совпадении → скриншот + запись инцидента
5. Отправка в ActivityWatch Server
6. Сохранение в PostgreSQL
7. Отображение в Grafana DLP Dashboard
```

### Сценарий 2: Пользователь посещает запрещенный сайт
```
1. Пользователь открывает сайт в браузере
2. Browser Collector определяет домен
3. Проверка по спискам и DLP правилам
4. Категоризация сайта
5. Отправка события в ActivityWatch
6. Отображение в WebUI Dashboard
```

### Сценарий 3: Мониторинг метрик
```
1. Prometheus Exporter опрашивает AW API
2. Сбор метрик (события, хосты, коллекторы)
3. Экспорт в формате Prometheus
4. Prometheus scrapes endpoint /metrics
5. Grafana строит графики
```
