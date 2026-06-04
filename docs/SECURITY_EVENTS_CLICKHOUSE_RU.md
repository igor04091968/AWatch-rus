# Агрегированные события безопасности через ClickHouse

## Назначение

`detmir-portal` может показывать краткую сводку событий безопасности за последние
24 часа из ClickHouse. Это дополнительный аналитический слой для управленческого
вывода, рисков подразделений, карты рисков и связи рисков с активностью.

Важно: этот режим не превращает AWatch-rus в SIEM. Портал не показывает сырые
журналы, не хранит SIEM-события как первичный источник и не создает инциденты
автоматически. В интерфейсе отображаются только агрегаты.

## Статус по умолчанию

Режим выключен:

```env
SECURITY_EVENTS_BACKEND=disabled
```

В выключенном режиме портал работает как раньше, `/api/reports` возвращает
валидный JSON, а `detmir-check` не проверяет ClickHouse.

## Переменные окружения

```env
SECURITY_EVENTS_BACKEND=clickhouse
CLICKHOUSE_URL=http://127.0.0.1:8123
CLICKHOUSE_DATABASE=analytics_1c
CLICKHOUSE_USER=default
CLICKHOUSE_PASSWORD=
```

`CLICKHOUSE_PASSWORD` используется только для HTTP Basic Auth и не выводится в
JSON, markdown-отчеты или health-ответы.

## Ожидаемые агрегаты

Портал формирует блок `security_events_summary`:

- `status`;
- `backend`;
- `events_24h`;
- `failed_logins_24h`;
- `suspicious_logins_24h`;
- `rdp_sessions_24h`;
- `account_changes_24h`;
- `agent_errors_24h`;
- `top_departments`;
- `last_event_utc`;
- `query_ms`;
- `fallback_used`.

Текущая реализация использует агрегированные запросы к таблицам
`entity_timeline` и `host_events` выбранной базы ClickHouse.

## Поведение при отказе

Если `SECURITY_EVENTS_BACKEND=clickhouse`, но ClickHouse недоступен:

- портал не падает;
- `/api/reports` остается валидным;
- `security_events_summary.fallback_used=true`;
- в ролевом представлении “Эксплуатация” показывается причина;
- `detmir-check` добавляет необязательное предупреждение, но не считает это
  критическим отказом контура.

## Где видно в портале

- “Сводка руководителя”: короткий счетчик событий за 24 часа.
- “Главный вывод”: события учитываются как один из подтверждающих слоев риска.
- “Риски подразделений”: события повышают приоритет подразделения только как
  агрегированный риск-фактор.
- “Карта рисков”: события отображаются отдельной колонкой.
- “Связь рисков и активности”: события участвуют в объяснении корреляции.
- “ИБ”: блок “События безопасности за 24 часа”.
- “Эксплуатация”: статус источника и причина fallback.

## Проверка

Выключенный режим:

```bash
SECURITY_EVENTS_BACKEND=disabled detmir-check --json
curl -s http://127.0.0.1:8720/portal/api/reports | jq '.security_events_summary'
```

Включенный режим:

```bash
SECURITY_EVENTS_BACKEND=clickhouse \
CLICKHOUSE_URL=http://127.0.0.1:8123 \
CLICKHOUSE_DATABASE=analytics_1c \
detmir-check --json
```

Ожидаемый результат при доступном ClickHouse: `security-events-clickhouse` имеет
`ok=true`, а `/api/reports.security_events_summary.backend="clickhouse"`.

Ожидаемый результат при недоступном ClickHouse: портал возвращает
`fallback_used=true`, а `detmir-check` показывает предупреждение
`security-events-clickhouse` с `required=false`.
