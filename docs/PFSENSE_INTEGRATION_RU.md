# pfSense Integration Readiness

## Статус Pilot v1.0

pfSense в Pilot v1.0 является опциональным интеграционным слоем сетевого
периметра. Полноценный SIEM не реализуется.

Текущий статус: `contract_only`.

Это означает:

- есть API-заготовка `/api/pfsense`;
- есть JSON-контракты для firewall events, vpn events, traffic summary и top destinations;
- есть демонстрационный fixture без реальных IP/hostname/login;
- нет заявления, что реальный ingestion включен;
- нет автоматического изменения firewall/VPN/routing;
- нет NAC, SOAR, quarantine и блокировок.

## Контракт событий

Минимальные поля firewall event:

- `timestamp`;
- `source_host`;
- `destination`;
- `action`;
- `protocol`;
- `rule_id`.

Минимальные поля VPN event:

- `timestamp`;
- `source_host`;
- `user_ref`;
- `action`;
- `tunnel`.

Traffic summary:

- период;
- количество событий;
- действия pass/block;
- объем трафика, если источник его предоставляет.

Top destinations:

- `destination`;
- `events`;
- `bytes`.

## API

`GET /api/pfsense` возвращает:

- `status`;
- `siem=false`;
- `ingestion_available`;
- `firewall_events`;
- `vpn_events`;
- `traffic_summary`;
- `top_destinations`;
- `schemas`;
- `demo_data_policy`.

Если реальный ingestion появится позже, статус должен измениться только после
проверки источника, свежести данных и отсутствия чувствительных значений в
demo-режиме.

## Demo fixture

Файл: [fixtures/pfsense-demo-events.json](fixtures/pfsense-demo-events.json).

Для демонстрации используются только специальные адресные диапазоны RFC 5737:

- `192.0.2.0/24`;
- `198.51.100.0/24`;
- `203.0.113.0/24`.

## Ограничения

AWatch-rus не является SIEM и не заменяет pfSense, NAC, SOAR или классический
DLP. pfSense-контракт нужен только для будущего обогащения Workforce Analytics
+ Security Analytics + Forensics сетевым контекстом.

Для Pilot v1.0 Dioxus не используется и не рассматривается.
