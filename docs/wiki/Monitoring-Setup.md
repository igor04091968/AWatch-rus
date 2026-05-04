# Monitoring Setup

Настройка Prometheus + Grafana стека для мониторинга ActivityWatch.

## Требования

- Docker & Docker Compose
- 2GB+ RAM
- 20GB+ disk space

## Установка через Docker Compose

```bash
cd grafana-1c
docker-compose up -d
```

## Компоненты

### Prometheus
Сбор и хранение метрик.

**Порт:** 9090
**Конфиг:** `prometheus/prometheus.yml`

### Grafana
Визуализация метрик и дашборды.

**Порт:** 3000
**Конфиг:** `grafana/provisioning/`

### SQL Exporter
Прямые SQL запросы к PostgreSQL.

**Порт:** 9398
**Конфиг:** `sql-exporter/`

### AW Exporter
Экспорт метрик ActivityWatch.

**Порт:** 9398
**Файл:** `sql-exporter/collectors/aw_activitywatch.py`

## Конфигурация Prometheus

```yaml
# prometheus/prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'activitywatch'
    static_configs:
      - targets: ['aw-exporter:9398']
    scrape_interval: 60s

  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']
```

## Конфигурация Grafana

### Datasources

```yaml
# grafana/provisioning/datasources/prometheus.yml
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
```

```yaml
# grafana/provisioning/datasources/postgres.yml
apiVersion: 1

datasources:
  - name: PostgreSQL
    type: postgres
    access: proxy
    url: postgres:5432
    database: activitywatch
    user: aw
    password: aw_password
```

### Dashboards

```yaml
# grafana/provisioning/dashboards/dashboards.yml
apiVersion: 1

providers:
  - name: 'ActivityWatch'
    orgId: 1
    folder: 'ActivityWatch'
    type: file
    disableDeletion: false
    updateIntervalSeconds: 10
    allowUiUpdates: true
    options:
      path: /etc/grafana/provisioning/dashboards
```

## Дашборды

### ActivityWatch Overview
- Активные хосты
- События по buckets
- Скорость поступления событий
- Статус коллекторов

**Файл:** `grafana/grafana/dashboards/aw_overview.json`

### DLP Incidents
- Количество инцидентов по типам
- Инциденты по пользователям
- Инциденты по серьезности
- Временные тренды

### Browser Monitoring
- Посещаемые домены
- Категории сайтов
- DLP нарушения

## Запуск

```bash
# Запуск всех сервисов
docker-compose up -d

# Проверка статуса
docker-compose ps

# Просмотр логов
docker-compose logs -f

# Остановка
docker-compose down
```

## Доступ

- **Grafana:** http://localhost:3000 (admin/admin)
- **Prometheus:** http://localhost:9090
- **AW Exporter:** http://localhost:9398/metrics

## Настройка алертов

### Prometheus Alerting Rules

```yaml
groups:
  - name: activitywatch_alerts
    rules:
      - alert: AWCollectorDown
        expr: aw_host_active == 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "ActivityWatch collector down on {{ $labels.host }}"

      - alert: AWExporterDown
        expr: aw_exporter_up == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "ActivityWatch exporter is down"
```

### Grafana Notification Channels

1. Откройте Grafana
2. Configuration → Alerting → Notification channels
3. Add new channel (Email, Slack, etc.)
4. Настройте правила алертов в дашбордах

## Мониторинг

### Ключевые метрики

**ActivityWatch:**
- `aw_bucket_events_total` - общее количество событий
- `aw_host_active` - активность хостов
- `aw_exporter_scrape_duration_seconds` - время scrape

**Prometheus:**
- `up` - статус целей
- `scrape_duration_seconds` - время сбора

**Grafana:**
- `grafana_statistic` - статистика Grafana

### Health Checks

```bash
# Prometheus
curl http://localhost:9090/-/healthy

# Grafana
curl http://localhost:3000/api/health

# AW Exporter
curl http://localhost:9398/health
```

## Backup

```bash
# Backup Grafana dashboards
docker exec grafana grafana-cli admin export-dashboard > dashboards_backup.json

# Backup Prometheus data
docker exec prometheus tar -czf /tmp/prometheus_data.tar.gz /prometheus
docker cp prometheus:/tmp/prometheus_data.tar.gz ./backup/
```

## Устранение проблем

### Grafana не доступна
```bash
# Проверьте контейнер
docker-compose ps grafana

# Проверьте логи
docker-compose logs grafana

# Перезапустите
docker-compose restart grafana
```

### Prometheus не собирает метрики
```bash
# Проверьте конфиг
docker exec prometheus promtool check config /etc/prometheus/prometheus.yml

# Проверьте цели
curl http://localhost:9090/api/v1/targets

# Проверьте логи
docker-compose logs prometheus
```

### AW Exporter не работает
```bash
# Проверьте соединение с AW Server
docker exec aw-exporter curl http://aw-server:5600/api/0/buckets

# Проверьте логи
docker-compose logs aw-exporter

# Проверьте метрики
curl http://localhost:9398/metrics
```

## Производительность

### Оптимизация Prometheus

```yaml
# prometheus/prometheus.yml
global:
  scrape_interval: 30s  # Увеличьте интервал
  evaluation_interval: 30s

storage:
  tsdb:
    retention.time: 30d  # Уменьшите retention
```

### Оптимизация Grafana

```bash
# Увеличьте память
# docker-compose.yml
grafana:
  environment:
    - GF_INSTALL_PLUGINS=
    - GF_SERVER_ROOT_URL=http://localhost:3000
    - GF_ANALYTICS_ENABLED=false
```

## Подробнее

- [Prometheus Exporter](Prometheus-Exporter)
- [Компоненты](Components)
