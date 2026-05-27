# Web Categorization

В `AWatch-rus` web-категоризация используется для того, чтобы browser/domain telemetry была пригодна не только для raw-логов, но и для DLP, worktime и управленческой аналитики.

## Источник данных

Основной Windows-компонент:

- `Browser Domains Collector`

События попадают в bucket-поток:

- `aw-detmir-web-category_<host>`

## Зачем нужна категоризация

Она позволяет:

- отделять рабочие ресурсы от нейтральных и личных
- строить DLP-правила по категориям
- использовать browser telemetry в worktime и management-отчётах
- давать понятный руководителю и ИБ срез, а не просто список доменов

## Кастомные правила

Для override/extension используется файл:

- `windows/web-category-rules.example.json`

Типовая группировка:

- `work`
- `neutral`
- `personal`

Правила задаются по доменам и root-domain логике.

## Связь с DLP policy

Web-категории используются вместе с `windows/dlp-policy.example.json`, где верхний `rules[]` может ссылаться на:

- `categoryGroups`
- конкретные `domains`
- временные окна (`hourFrom` / `hourTo`)

## Практическая граница

Текущий browser path в первую очередь:

- наблюдающий
- аналитический
- policy-aware

То есть категоризация уже production-usable, но не равна полноценному inline secure web gateway.

## Канонические документы

- [Browser Domains Monitoring](Browser-Domains-Monitoring)
- [Пример category rules](../../windows/web-category-rules.example.json)
- [Пример DLP policy](../../windows/dlp-policy.example.json)
