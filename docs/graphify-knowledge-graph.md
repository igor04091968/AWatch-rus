# ActivityWatch-Russian: Knowledge Graph Documentation

## Что такое Knowledge Graph?

**Knowledge Graph** (граф знаний) - это визуальное представление связей между компонентами программного проекта. Для ActivityWatch-Russian граф показывает:

- **Функции и модули** как узлы (nodes)
- **Вызовы и зависимости** как связи (edges)
- **Кластеры** как сообщества связанных функций (communities)

## Зачем это нужно?

1. **Понимание архитектуры** - увидеть структуру проекта без чтения кода
2. **Поиск зависимостей** - понять, какие компоненты влияют друг на друга
3. **Выявление изоляции** - найти функции, которые не связаны с остальным кодом
4. **Документация** - автоматическая генерация обзора системы

## Как был построен граф?

Граф построен с помощью инструмента **graphify** методом **AST extraction**:

1. **Сканирование файлов** - найдено 29 кодовых файлов (PowerShell, Python, JavaScript)
2. **Анализ синтаксиса** - извлечены функции, классы, вызовы
3. **Построение графа** - 404 узла (функции), 933 связи (вызовы)
4. **Кластеризация** - 27 сообществ по схожести связей

## Структура проекта по сообществам

### 1. DLP Endpoint Monitoring (62 nodes)
**Мониторинг конечных точек DLP**

Функции для отслеживания:
- **Clipboard** - буфер обмена
- **Print** - задания на печать
- **USB** - запись на USB накопители

Ключевые файлы:
- `windows/dlp-endpoint-signals-collector.ps1`
- `windows/dlp-policy-test.json`

### 2. WebUI Russian Localization & Patches (56 nodes)
**Русификация веб-интерфейса**

Патчи для ActivityWatch WebUI:
- Перевод интерфейса на русский язык
- Скрытие лишних элементов навигации
- Инъекция стилей для RU локали
- Поддержка кириллицы

Ключевые файлы:
- `aw-server/aw-ru-patch.js`
- `aw-server/aw-sw-cleanup.js`

### 3. Browser Domains Monitoring (54 nodes)
**Мониторинг доменов браузеров**

Отслеживание посещаемых сайтов:
- Определение домена из URL
- Категоризация веб-ресурсов
- Проверка DLP правил для доменов
- Скриншоты при нарушениях

Ключевые файлы:
- `windows/browser-domains-native-collector.ps1`

### 4. DLP Events Aggregation (38 nodes)
**Агрегация событий DLP**

Сбор и обработка инцидентов:
- Чтение событий из ActivityWatch buckets
- Нормализация данных
- Запись в PostgreSQL
- Генерация отчетов

Ключевые файлы:
- `scripts/aggregate_dlp_events.py`

### 5. DLP Review Center (34 nodes)
**Центр просмотра инцидентов DLP**

WebUI компоненты для:
- Просмотра списка инцидентов
- Управления правилами DLP
- Архивирования событий
- Фильтрации по хостам

### 6. Email Outbound Monitoring (34 nodes)
**Мониторинг исходящей почты**

Отслеживание email:
- Outlook Sent Items
- SMTP соединения
- Проверка DLP правил для email
- Блокировка нарушений

Ключевые файлы:
- `windows/email-outbound-collector.ps1`

### 7. Prometheus Metrics Exporter (12 nodes)
**Экспорт метрик для Prometheus**

Сбор метрик ActivityWatch:
- Количество событий по buckets
- Активность хостов
- Статистика collectors
- HTTP endpoint для Prometheus

Ключевые файлы:
- `grafana-1c/sql-exporter/collectors/aw_activitywatch.py`

### 8. pfSense Firewall Integration (12 nodes)
**Интеграция с pfSense**

Сбор данных с firewall:
- HTTP API опрос
- Парсинг логов pfSense
- Нормализация данных
- Отправка в ActivityWatch

Ключевые файлы:
- `pfsense/pfsense-aw-poller.py`

### 9. Migration Scripts (14 nodes)
**Скрипты миграции**

Обновление путей и конфигураций:
- Перенос данных между версиями
- Обновление конфигурационных файлов
- Конвертация путей

Ключевые файлы:
- `windows/migrate-awatch-rus-paths.ps1`

### 10. Worktime Session Tracking (10 nodes)
**Отслеживание рабочих сессий**

Учет рабочего времени:
- Определение начала/конца сессии
- Учет перерывов
- Агрегация по дням

Ключевые файлы:
- `windows/worktime-session-collector.ps1`

### 11. Deployment Automation (6 nodes)
**Автоматизация развертывания**

Скрипты деплоя:
- `windows/deploy-domain-users.ps1` - доменная развертка
- `windows/deploy-single-user.ps1` - одиночный пользователь
- `windows/deploy-ensemble.ps1` - групповое развертывание

## Как пользоваться графом?

### Интерактивная визуализация
Откройте файл `graphify-out/index.html` в браузере:

- **Zoom** - колесо мыши
- **Pan** - перетаскивание
- **Click node** - детали узла
- **Search** - поиск по названию функции

### Фильтрация по сообществам
Каждое сообщество имеет свой цвет:
- Синий - DLP мониторинг
- Зеленый - WebUI патчи
- Красный - Collectors
- Желтый - Утилиты

### Поиск зависимостей
1. Найдите функцию в графе
2. Посмотрите на исходящие связи (что вызывает)
3. Посмотрите на входящие связи (кто вызывает)

## Интерпретация связей

### Высокая связность (hub nodes)
Функции с большим количеством связей:
- `*_get_deploymentconfig` - чтение конфигурации
- `*_invoke_awjsonpost` - отправка данных в ActivityWatch
- `*_ensure_bucket` - создание bucket

### Изолированные компоненты
Маленькие сообщества (1-3 nodes) могут быть:
- Утилитными функциями
- Зависимостями от внешних библиотек
- Устаревшим кодом

## Статистика проекта

| Метрика | Значение |
|---------|----------|
| Всего файлов | 29 кодовых файлов |
| Всего функций | 404 |
| Всего связей | 933 |
| Сообществ | 27 |
| Средний размер сообщества | 15 nodes |
| Самое большое сообщество | 62 nodes (DLP Endpoint) |

## Технологии по типам файлов

- **PowerShell (.ps1)** - Windows collectors, deployment
- **Python (.py)** - aggregation, exporters, pfSense integration
- **JavaScript (.js)** - WebUI patches
- **JSON** - конфигурации, policies

## Рекомендации по архитектуре

### Сильные стороны
1. **Четкая модульность** - каждый collector в своем сообществе
2. **Изоляция DLP** - отдельные компоненты для разных типов мониторинга
3. **Унификация** - общие паттерны в collector'ах

### Возможные улучшения
1. **Дублирование** - несколько сообществ с похожими функциями (WebUI patches)
2. **Интеграция** - слабые связи между некоторыми компонентами
3. **Документация** - не все функции имеют явные назначения

## Обновление графа

Для пересборки графа после изменений кода:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
graphify .
```

Для инкрементального обновления (только измененные файлы):

```bash
graphify . --update
```

## Полезные запросы

### Найти путь между функциями
```bash
graphify path "dlp_endpoint_signals_collector_get_deploymentconfig" "aggregate_dlp_events_main"
```

### Объяснить функцию
```bash
graphify explain "browser_domains_native_collector_get_hostfromurl"
```

### Поиск по вопросу
```bash
graphify query "Как работает мониторинг clipboard?"
```

## Ссылки

- **Интерактивный граф**: `graphify-out/index.html`
- **Отчет**: `graphify-out/GRAPH_REPORT.md`
- **JSON граф**: `graphify-out/graph.json`
- **Graphify документация**: https://github.com/brevity-x/graphify
