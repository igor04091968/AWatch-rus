# Grafana dashboard catalog for file-based 1C

Ниже — целевой набор дашбордов. Это не “один giant dashboard”, а иерархия под
разные роли.

## 1. 1C Executive Summary

Для руководства и владельца процесса.

Панели:

- Выручка сегодня / 7 дней / 30 дней
- Просроченная дебиторка
- Возвраты и корректировки
- Открытые cases
- High/Critical detections
- Топ проблемных баз
- Топ риск-пользователей
- Тренд аномалий

Основные таблицы:

- `documents`
- `detections`
- `cases`

## 2. 1C Operations Health

Для сопровождения и инфраструктуры.

Панели:

- CPU / RAM / Disk
- Disk latency
- Свободное место
- RDP sessions
- Ошибки SMB / файлового доступа
- Ошибки журнала регистрации
- Длительные операции
- Backup freshness

Основные таблицы:

- `host_events`
- `reglog_events`

## 3. 1C Audit Overview

Для контроля, ИБ и внутреннего аудита.

Панели:

- Входы по пользователям
- Входы вне рабочего времени
- Изменения критичных объектов
- Массовые перепроведения
- Нетипичные корректировки
- Severity split
- Топ пользователей по risk score

Основные таблицы:

- `reglog_events`
- `audit_events`
- `detections`

## 4. 1C Detections

Для triage и контроля rules.

Панели:

- Detections by severity
- Top rules
- Cases opened by day
- Open detections
- Critical timeline
- Top entities

Основные таблицы:

- `detections`
- `cases`

## 5. 1C Investigation Timeline

Для ручного расследования.

Панели:

- Entity timeline
- Case drilldown
- User trace
- Document trace
- Related detections
- Raw event details

Основные таблицы:

- `entity_timeline`
- `detections`
- `cases`
- `reglog_events`
- `audit_events`
- `documents`

## 6. 1C Data Quality

Для контроля самого пайплайна.

Панели:

- Последняя выгрузка по каждой базе
- ETL lag
- Битые/пустые файлы
- Rows loaded per batch
- Ошибки ETL

Источники:

- ETL run log
- архив landing/ETL статусов

## Единые variables

Во все dashboards:

- `infobase`
- `organization`
- `user`
- `host`
- `document_type`
- `severity`
- `case_status`
- `time range`
