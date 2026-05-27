# DLP Rules

Базовая DLP-политика в `AWatch-rus` задаётся JSON-файлом:

- `windows/dlp-policy.example.json`

Это основной policy contract для endpoint- и web/DLP-логики.

## Структура политики

Типовой файл содержит разделы:

- `defaults`
- `rules`
- `endpoint`
- `contentAnalysis`
- `ioc`

## defaults

Глобальные параметры по умолчанию:

- `enabled`
- `cooldownSeconds`
- `action`
- `severity`

## rules

Верхний `rules[]` используется в первую очередь для web/domain сценариев:

- `domains`
- `categoryGroups`
- `hourFrom`
- `hourTo`
- `message`
- `action`
- `severity`

Это полезно для:

- личных сайтов в рабочее время
- облачных хранилищ
- anonymizer/VPN web-path

## endpoint

Endpoint-правила разделены по каналам:

- `endpoint.clipboard[]`
- `endpoint.usb[]`
- `endpoint.print[]`
- `endpoint.email[]`

Типовые поля:

- `id`
- `enabled`
- `cooldownSeconds`
- `action`
- `severity`
- `message`

Дополнительные условия зависят от канала:

- `regexPatterns`, `minLength` — для clipboard
- `documentRegex` — для print
- `subjectRegex`, `recipientRegex`, `attachmentRegex`, `externalOnly` — для email

## contentAnalysis

Этот раздел управляет server-side content analysis контуром:

- `dictionaryPack`
- `regexPack`
- `ocrEnabled`

## ioc

IOC-слой позволяет подтягивать внешние индикаторы:

- `enabled`
- `source`
- `format`
- `refreshMinutes`

## Действия

На практике используются:

- `log`
- `alert`
- `block`

Важно различать:

- `alert` — зафиксировать и эскалировать
- `block` — реально ограничить действие, если канал это поддерживает

Полный `block` уже есть для части endpoint/email каналов, но не превращает браузерный путь в полноценный inline web-gateway.

## Канонические документы

- [Пример policy](../../windows/dlp-policy.example.json)
- [DLP Endpoint Monitoring](DLP-Endpoint-Monitoring)
- [Email Outbound Monitoring](Email-Outbound-Monitoring)
- [Категоризация сайтов](Web-Categorization)
