# Prometheus Exporter

`AWatch-rus` использует Prometheus-compatible exporter path для operational monitoring и Grafana.

Назначение контура простое:

- собрать health и activity-метрики из `ActivityWatch`
- отдать их в формате Prometheus
- использовать дальше в `Prometheus -> Grafana -> alerts`

## Что экспортируется

Типовые группы метрик:

- состояние bucket-источников
- активность хостов
- collector heartbeat
- exporter health
- временные метки последней активности

## Основные endpoint'ы

Обычно используются:

- `/metrics`
- `/health`

## Где применяется

Exporter нужен для:

- Grafana operational dashboards
- внешнего мониторинга доступности
- алертинга по деградации collector/runtime path
- E2E health checks

## Практический контур

В репозитории monitoring path уже связан с:

- `Prometheus`
- `Grafana`
- `AW-rus health checks`
- `check-aw-full.sh`

То есть это не “метрики ради метрик”, а часть рабочего operational контроля.

## Канонические документы

- [Monitoring Setup](Monitoring-Setup)
- [Компонентная диаграмма exporter](../diagrams/prometheus-exporter.md)
- [Компоненты системы](Components)
