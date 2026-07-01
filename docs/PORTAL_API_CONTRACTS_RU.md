# API-контракты портала AWatch-rus

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

`GET /api/reports` должен сохранять additive payload `workforce_operations`.
Это основной contract для экрана руководителя по загрузке, простоям, перегрузу,
дисциплине процесса и достоверности данных. Клиент должен читать:

- `workforce_operations.summary`;
- `workforce_operations.rows`;
- `workforce_operations.model`;
- `workforce_operations.rows[].load_status`;
- `workforce_operations.rows[].idle_status`;
- `workforce_operations.rows[].discipline_status`;
- `workforce_operations.rows[].data_confidence`;
- `workforce_operations.rows[].recommended_action`.

Подробная семантика статусов:
[WORKFORCE_OPERATIONS_MODEL_RU.md](WORKFORCE_OPERATIONS_MODEL_RU.md).

`GET /api/reports` также публикует additive payload `modules.dlp`.
Клиент должен трактовать его как runtime capability, а не как claim
сертифицированной DLP:

- `modules.dlp.enabled`;
- `modules.dlp.status`;
- `modules.dlp.hot_path`;
- `modules.dlp.note`.

Если `modules.dlp.enabled=false`, Workforce UI должен продолжать работу и
показывать DLP/Security/Forensics как disabled или not configured, не превращая
это в ошибку основного рабочего экрана.

`GET /api/operator` также публикует additive runtime-state поля для первичного
экрана:

- `cache_status`;
- `modules.dlp.enabled`;
- `modules.dlp.status`;
- `modules.dlp.hot_path`;
- `modules.dlp.note`;
- `summary.severity`;
- `summary.blocks`.

Если `cache_status=warming`, клиент должен показать bounded stale/warming
state и не держать бесконечный loading indicator. Если
`modules.dlp.enabled=false`, operator screen должен считать DLP disabled-state
допустимым состоянием, а не ошибкой Workforce core.

## Что не меняется

- HTML-портал не удаляется.
- Маршрут `/portal/` остаётся стабильным.
- Backend-расчёты, JSON-хранилища и workflow не дублируются во frontend.
- Публичные JSON-поля не переименовываются без новой версии контракта.
- Экспериментальные mirror/prototype направления не входят в публичный
  contract layer и не должны возвращаться без отдельного architecture decision.

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
