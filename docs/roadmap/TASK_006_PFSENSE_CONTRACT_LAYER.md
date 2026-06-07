# TASK 006: pfSense Contract Layer

## Цель

Сохранить pfSense направление как contract/readiness layer без ложного
позиционирования AWatch-rus как SIEM.

## Объем

- Контракты firewall events.
- Контракты VPN events.
- Traffic summary и top destinations как модель данных.
- Документация статуса `contract_only`.

## Ограничения

- Не заявлять production ingestion, если он не реализован.
- Не делать полноценный SIEM.
- Не добавлять новые collectors в рамках этой задачи.

## Результат

pfSense readiness описана как подготовленный контрактный слой, пригодный для
будущей интеграции без завышенных продуктовых обещаний.
