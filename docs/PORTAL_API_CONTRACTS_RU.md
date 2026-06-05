# API-контракты портала DetMir

## Цель

Закрепить стабильный слой API для текущего HTML-портала и будущего
React/Tauri-интерфейса без переписывания backend-логики.

Текущий HTML-портал остаётся рабочим и основным. Новый UI должен подключаться к
тем же endpoint-ам через documented contract и не должен парсить HTML.

## Контрактные endpoint-ы

Основной портал:

- `GET /api/contracts`
- `GET /api/contracts/openapi.json`
- `GET /api/contracts/typescript.d.ts`

## Правила совместимости

- Изменения API должны быть additive.
- Нельзя удалять существующие поля без отдельной migration window.
- Клиенты обязаны игнорировать неизвестные поля.
- Клиенты обязаны корректно обрабатывать отсутствующие optional-поля.
- Поля с `null` не должны ломать UI.
- Ошибки API должны отображаться пользователю на русском языке.
- Breaking changes требуют отдельного version bump контракта и architecture
  decision.
- Будущий React/Tauri UI не должен парсить HTML-страницы, CSS или встроенный
  JavaScript текущего портала как источник данных.
- Бизнес-логика остаётся на стороне Rust backend/API; frontend только
  отображает и отправляет пользовательские действия через контрактные endpoint-ы.

## Минимальный набор для будущего React/Tauri UI

Для первого production-grade клиента достаточно:

- `GET /api/contracts` - discovery и версия контракта;
- `GET /api/reports` - главный управленческий payload;
- `GET /api/operator` - обзор портала;
- `GET /api/incidents` - проверки и события;
- `GET /api/cases` - расследования;
- `POST /api/incident-review` - ручное решение по кандидату;
- `POST /api/cases` - ручное создание дела;
- `GET /api/investigation-pack/{candidate_id}` - пакет расследования;
- `GET /api/readiness/latest` - готовность системы;
- `GET /api/workforce/policy/explain` - объяснение расчёта показателей.

## Что не меняется

- HTML-портал не удаляется.
- Маршрут `/portal/` остаётся стабильным.
- Backend-расчёты, JSON-хранилища и workflow не дублируются во frontend.
- Публичные JSON-поля не переименовываются без новой версии контракта.
- Dioxus и DPD Portal исключены из архитектурного roadmap проекта.

## Проверка

```bash
curl -sS http://127.0.0.1:8720/api/contracts | jq .
curl -sS http://127.0.0.1:8720/api/contracts/openapi.json | jq .openapi
curl -sS http://127.0.0.1:8720/api/contracts/typescript.d.ts | head
```

Ожидаемый результат:

- `ok=true`;
- `contract_version` заполнен;
- OpenAPI JSON валиден;
- TypeScript declarations доступны;
- будущий React/Tauri UI может использовать JSON API без HTML-парсинга.
