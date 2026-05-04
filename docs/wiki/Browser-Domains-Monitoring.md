# Browser Domains Monitoring

Мониторинг посещаемых доменов в браузерах с категоризацией и проверкой DLP правил.

## Обзор

Browser Domains Collector отслеживает:
- Посещаемые веб-сайты
- Категоризацию доменов
- DLP проверку запрещенных сайтов
- Скриншоты при нарушениях

## Архитектура

```
Browser → Window Detection → URL Extraction → Domain Parsing → Categorization → DLP Check → ActivityWatch
```

## Функции

### Domain Detection
- Определение активного окна браузера
- Извлечение URL из заголовка окна
- Парсинг домена из URL
- Определение корневого домена

### Categorization
- Кастомные правила категорий
- Web Category API (опционально)
- Кэширование результатов
- Автоопределение категорий

### DLP Checking
- Проверка по спискам доменов
- DLP правила для доменов
- Временные окна
- Блокировка нарушений

## Поддерживаемые браузеры

- Google Chrome
- Mozilla Firefox
- Microsoft Edge
- Opera
- Яндекс.Браузер

## Конфигурация

### Custom Category Rules
```json
{
  "custom_categories": {
    "social_media": [
      "*.facebook.com",
      "*.twitter.com",
      "*.instagram.com"
    ],
    "news": [
      "*.news.com",
      "*.media.com"
    ],
    "blocked": [
      "*.malware.com",
      "*.phishing.com"
    ]
  }
}
```

### DLP Domain Rules
```json
{
  "domain_rules": [
    {
      "pattern": "*gambling*",
      "category": "gambling",
      "action": "alert",
      "severity": "medium"
    },
    {
      "pattern": "*adult*",
      "category": "adult",
      "action": "block",
      "severity": "high"
    }
  ]
}
```

## События

### Browsing Event
```json
{
  "timestamp": "2024-01-01T12:00:00Z",
  "type": "browsing",
  "data": {
    "url": "https://www.example.com/page",
    "domain": "example.com",
    "root_domain": "example.com",
    "category": "technology",
    "browser": "chrome.exe",
    "title": "Example Page Title",
    "user": "user1",
    "host": "WORKSTATION01"
  }
}
```

### DLP Incident Event
```json
{
  "timestamp": "2024-01-01T12:00:00Z",
  "type": "dlp_incident",
  "source": "browser_domain",
  "rule_id": "blocked_domains",
  "severity": "high",
  "data": {
    "url": "https://blocked.com",
    "domain": "blocked.com",
    "category": "malicious",
    "screenshot": "path/to/screenshot.png"
  }
}
```

## Производительность

### Оптимизации
- Кэширование категорий (TTL 24h)
- Batch запросы к category API
- Debouncing быстрых переходов
- Асинхронная отправка событий

## Мониторинг

- Количество уникальных доменов
- Распределение по категориям
- Частота DLP инцидентов
- Кэш hit rate

## Подробнее

- [Детальная диаграмма](https://github.com/igor04091968/AWatch-rus/blob/main/docs/diagrams/browser-domains-monitoring.md)
- [Категоризация сайтов](Web-Categorization)
