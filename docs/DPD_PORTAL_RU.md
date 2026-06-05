# DPD Parallel Portal

`detmir-dpd-portal` - параллельный gateway-портал DetMir. Он не заменяет
текущий `detmir-portal`, а повторяет его функциональность через отдельный
маршрут `/dpd/`.

## Назначение

DPD нужен для безопасной эволюции интерфейса:

- основной HTML-портал `/portal/` остаётся стабильным;
- `/dpd/` работает как полный mirror текущего портала;
- будущий React/Tauri UI сможет использовать те же API-контракты;
- новые UI-решения проверяются без cutover и без дублирования бизнес-логики.

DPD проксирует:

- вкладки;
- API;
- действия проверки;
- дела;
- расследования;
- markdown/download endpoints;
- материалы проверки.

DPD gateway сохраняет операторский контекст для симметрии с основным порталом:
`X-Remote-User`, `X-Gateway-User`, `X-Forwarded-*`, `User-Agent`, `Referer`,
`Origin`, `Cookie` и `Authorization` передаются в upstream, hop-by-hop
заголовки не передаются. Это нужно, чтобы audit/review/case-действия через
`/dpd/` фиксировались так же, как через `/portal/`.

## Архитектурное решение

Выбран зрелый путь:

```text
detmir-portal Rust backend
        |
        | stable /api/contracts
        v
current HTML UI  +  DPD mirror  +  future React/Tauri UI
```

Не используется отдельный экспериментальный UI-фреймворк в production path.
Сначала фиксируются API-контракты, тестируется совместимость и только затем
добавляется новый frontend.

## Запуск

```bash
detmir-dpd-portal \
  --bind 127.0.0.1:8722 \
  --upstream-base http://127.0.0.1:8720
```

Переменные окружения:

- `DETMIR_DPD_BIND`;
- `DETMIR_DPD_UPSTREAM_BASE`;
- `DETMIR_DPD_TIMEOUT_SECONDS` - по умолчанию 60 секунд, чтобы первый
  холодный `/api/reports` после рестарта не выглядел как отказ DPD-портала.

## Маршруты

- `/dpd/` - полный параллельный mirror текущего портала.
- `/dpd/_dpd/health` - health самого DPD gateway.
- `/dpd/preview/` - компактный read-only preview-экран.

## API-контракты

Контракты публикует основной `detmir-portal`, а DPD зеркалирует их:

- `/api/contracts`;
- `/api/contracts/openapi.json`;
- `/api/contracts/typescript.d.ts`;
- `/dpd/api/contracts`;
- `/dpd/api/contracts/openapi.json`;
- `/dpd/api/contracts/typescript.d.ts`.

Правило совместимости: изменения API должны быть additive. Клиенты React/Tauri
обязаны игнорировать неизвестные поля и корректно обрабатывать отсутствующие
optional-поля.

## Проверка симметрии

Минимальная проверка на сервере:

```bash
systemctl is-active detmir-dpd-portal detmir-portal nginx --no-pager
curl -sS http://127.0.0.1:8722/_dpd/health
curl -sS -o /dev/null -w 'dpd_index=%{http_code}\n' http://127.0.0.1:8722/
curl -sS -o /dev/null -w 'dpd_reports=%{http_code}\n' http://127.0.0.1:8722/api/reports
curl -sS -o /dev/null -w 'dpd_contracts=%{http_code}\n' http://127.0.0.1:8722/api/contracts
```

Браузерная проверка с ноутбука:

```bash
ssh -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 -o ServerAliveCountMax=3 \
  -N -L 18720:127.0.0.1:8720 <GATEWAY_HOST>

detmir-dpd-portal \
  --bind 127.0.0.1:18722 \
  --upstream-base http://127.0.0.1:18720

DETMIR_PORTAL_SMOKE_URL=http://127.0.0.1:18722/ \
DETMIR_PORTAL_SMOKE_TIMEOUT_MS=70000 \
node scripts/detmir-portal-tabs-smoke.mjs
```

Ожидаемый результат: `ok=true`, все вкладки работают, нет JS/API ошибок,
абсолютные `/portal/...` ссылки в HTML/JS переписаны в `/dpd/...`.

## Ограничения

- DPD gateway не добавляет новые бизнес-сущности.
- DPD gateway не заменяет текущий портал.
- Данные берутся из существующего портала/API.
- Новый React/Tauri UI должен появляться поверх контрактов, а не через
  копирование backend-логики.
