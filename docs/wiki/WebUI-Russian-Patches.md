# WebUI Русификация

Система патчей для русификации и адаптации ActivityWatch WebUI под российские требования.

## Обзор

WebUI патчи добавляют:
- Полный перевод интерфейса на русский
- DLP центр для просмотра инцидентов
- Центр управления правилами
- Группы хостов
- Русские стили и шрифты

## Архитектура

```
Original WebUI → Patch Loader → DOM Injection → Style Injection → Translation → RU WebUI
```

## Компоненты патчей

### Navigation Patches
- Скрытие лишних пунктов меню
- Добавление DLP навигации
- Обновление ссылок

### DLP Center Injection
- Центр просмотра инцидентов
- Менеджер правил
- Центр алертов
- Архивирование

### Host Groups Center
- Управление группами хостов
- Визуализация по группам
- Агрегированная статистика

### Translation
- Перевод всех текстовых элементов
- Русские форматы дат/чисел
- Кириллические шрифты

## Установка

```bash
# Применение патчей
node aw-server/aw-ru-patch.js

# Очистка старых патчей
node aw-server/aw-sw-cleanup.js
```

## Конфигурация

### Host Groups Config
```json
{
  "host_groups": [
    {
      "id": "workstations",
      "name": "Рабочие станции",
      "hosts": ["WORKSTATION01", "WORKSTATION02"],
      "color": "#4ecdc4"
    },
    {
      "id": "servers",
      "name": "Серверы",
      "hosts": ["SERVER01", "SERVER02"],
      "color": "#ff6b6b"
    }
  ]
}
```

### DLP Settings
```json
{
  "dlp_enabled": true,
  "review_center_enabled": true,
  "alerts_center_enabled": true,
  "auto_refresh_interval": 30,
  "default_severity_filter": "all"
}
```

## Функции WebUI

### DLP Review Center
- Список инцидентов
- Фильтрация по типам
- Детальный просмотр
- Архивирование
- Экспорт

### DLP Rules Manager
- Список правил
- Создание/редактирование
- Включение/выключение
- Тестирование правил

### Host Groups Center
- Управление группами
- Визуализация
- Статистика по группам

## Совместимость

- ActivityWatch v0.12.x - v0.14.x
- Chrome 90+, Firefox 88+, Edge 90+, Safari 14+

## Производительность

- Lazy loading патчей
- Кэширование переводов
- Debouncing обновлений DOM
- Virtual scrolling

## Отладка

```javascript
// Режим разработки
const DEBUG_MODE = true;

// Логирование патчей
console.log('[AW-RU-Patch] Applying patch:', patchName);
```

## Подробнее

- [Детальная диаграмма](https://github.com/igor04091968/AWatch-rus/blob/main/docs/diagrams/webui-patches.md)
- [Группы хостов](Host-Groups)
