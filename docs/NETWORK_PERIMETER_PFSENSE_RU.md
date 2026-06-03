# Network perimeter и pfSense

Документ описывает роль pfSense в архитектуре DetMir/AWatch-rus для
коммерческих внедрений и экспертной оценки. pfSense рассматривается как
опциональный интеграционный слой сетевого периметра, а не как обязательная
часть продукта.

## 1. Позиция продукта

DetMir/AWatch-rus поставляет:

- сбор и нормализацию endpoint/server telemetry;
- Workforce analytics;
- технический аудит;
- DLP-lite/ИБ evidence workflow;
- readiness checks и portal reporting.

Сетевой шлюз, firewall, NAT, VPN и quarantine enforcement могут быть
интегрированы с DetMir, но не входят в минимальный состав продукта.

## 2. Роль pfSense

pfSense может использоваться как:

- источник сетевого контекста;
- внешний policy enforcement point;
- шлюз для ограничений VLAN/alias/rules;
- источник логов для корреляции с endpoint activity.

DetMir не требует pfSense для базовой работы портала, readiness, workforce,
DLP-lite evidence и отчетов.

## 3. Интеграционные границы

| Слой | Статус |
|---|---|
| Endpoint telemetry | обязательный слой DetMir |
| Server-side checks/readiness | обязательный слой DetMir |
| Portal/Grafana/Prometheus | обязательный слой DetMir |
| pfSense logs/context | опциональная интеграция |
| pfSense policy enforcement | опциональная интеграция с отдельным решением |
| Автоматический quarantine | не включать без отдельного согласования |

## 4. Безопасный режим внедрения

По умолчанию:

- DetMir только читает сетевой контекст, если интеграция включена;
- любые изменения firewall/NAT/VPN/quarantine запрещены без отдельного change
  request;
- pfSense credentials хранятся вне Git и вне public release assets;
- public docs используют placeholders и TEST-NET адреса.

## 5. Что не заявлять

Для реестра российского ПО и публичной экспертизы не позиционировать DetMir как:

- firewall;
- NAC;
- VPN gateway;
- сертифицированное средство сетевой защиты;
- обязательный модуль управления pfSense.

Корректная формулировка:

> DetMir поддерживает интеграцию с сетевым периметром заказчика, включая
> pfSense-compatible gateways, как внешний источник контекста и опциональную
> точку применения политик.

## 6. Будущий roadmap

Возможные этапы развития:

1. Read-only import сетевых событий.
2. Корреляция endpoint activity и VPN/network context.
3. Manual approval workflow для сетевых ограничений.
4. Controlled enforcement через allowlist политик.
5. Audit trail каждого сетевого действия.

До отдельного решения владельца продукта pfSense runtime остается no-touch.
