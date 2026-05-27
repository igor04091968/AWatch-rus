# Email Outbound Monitoring

Мониторинг исходящей почты в `AWatch-rus` реализован отдельным Windows-коллектором:

- `windows/email-outbound-collector.ps1`

Это production-компонент DLP-контура, а не экспериментальный proof-of-concept.

## Что собирается

Поддерживаются два режима:

- `outlook` — через Outlook COM и папку `Sent Items`
- `smtp` — через наблюдение SMTP/TCP-соединений
- `both` — оба режима одновременно

Коллектор пишет:

- `aw-email-monitor_<host>` — email-события и heartbeat
- `aw-dlp-incidents_<host>` — инциденты при срабатывании DLP-правил

## Что умеет DLP

Email-правила живут в секции:

- `endpoint.email[]`

Поддерживаются типовые условия:

- `subjectRegex`
- `recipientRegex`
- `senderRegex`
- `attachmentRegex`
- `minAttachments`
- `minBodyLength`
- `externalOnly`
- `internalDomain`

Поддерживаемые действия:

- `alert`
- `block`

Практическая граница важна:

- полноценный `block` работает в `Outlook mode`
- в `SMTP mode` система честно фиксирует инцидент и уведомление, но не делает inline network-block

## Где используется

Компонент нужен для:

- DLP по исходящей почте
- расследований и case path
- ИБ-дашбордов
- operator review

## Развёртывание

Коллектор рассчитан на user-session запуск и обычно входит в Windows deployment toolkit:

- `windows/deploy-single-user.ps1`
- `windows/deploy-domain-users.ps1`
- `windows/ActivityWatch.Windows.Common.psm1`

## Канонические документы

- [Основной документ по коллектору](../email-outbound-collector.md)
- [Установка Windows collectors](Windows-Installation)
- [DLP Правила](DLP-Rules)
