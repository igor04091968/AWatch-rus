# Network perimeter и pfSense

Документ описывает роль pfSense в архитектуре AWatch-rus для
коммерческих внедрений и экспертной оценки. Для Pilot v1 pfSense
рассматривается только как `contract_only/readiness`: контракт данных, fixture
и API-заготовка без заявления production ingestion или управления сетевыми
политиками.

## 1. Позиция продукта

AWatch-rus поставляет:

- сбор и нормализацию endpoint/server telemetry;
- Workforce analytics;
- технический аудит;
- DLP-lite/ИБ evidence workflow;
- readiness checks и portal reporting.

Сетевой шлюз, firewall, NAT, VPN и quarantine enforcement являются внешним
периметром. Их можно рассматривать как будущие интеграционные направления, но
они не входят в приемочный контур Pilot v1 и не заявляются как реализованный
runtime.

## 2. Роль pfSense

pfSense может использоваться как:

- будущий источник сетевого контекста;
- будущий внешний policy enforcement point после отдельного change request;
- будущий источник логов для корреляции с endpoint activity.

AWatch-rus не требует pfSense для базовой работы портала, readiness, workforce,
DLP-lite evidence и отчетов.

## 3. Интеграционные границы

| Слой | Статус |
|---|---|
| Endpoint telemetry | обязательный слой AWatch-rus |
| Server-side checks/readiness | обязательный слой AWatch-rus |
| Portal/API/report layer | обязательный слой Pilot v1 |
| pfSense contracts/fixtures/API-заготовка | `contract_only/readiness` |
| pfSense logs/context production ingestion | не заявляется для Pilot v1 |
| pfSense policy enforcement | future, только с отдельным решением |
| Автоматический quarantine | не включать без отдельного согласования |

## 4. Безопасный режим внедрения

По умолчанию в Pilot v1:

- AWatch-rus показывает только контрактный readiness-слой pfSense;
- production ingestion, если он появится позже, должен включаться отдельным
  решением после проверки источника, свежести данных и sanitization;
- любые изменения firewall/NAT/VPN/quarantine запрещены без отдельного change
  request;
- pfSense credentials хранятся вне Git и вне public release assets;
- public docs используют placeholders и TEST-NET адреса.

## 5. Что не заявлять

Для реестра российского ПО и публичной экспертизы не позиционировать AWatch-rus как:

- firewall;
- NAC;
- VPN gateway;
- сертифицированное средство сетевой защиты;
- обязательный модуль управления pfSense.

Корректная формулировка:

> AWatch-rus архитектурно предусматривает интеграцию с сетевым периметром
> заказчика, включая pfSense-compatible gateways. В Pilot v1 этот слой имеет
> статус `contract_only/readiness` и не является production ingestion или
> механизмом изменения сетевых политик.

## 6. Будущий roadmap

Возможные этапы развития:

1. Read-only import сетевых событий.
2. Корреляция endpoint activity и VPN/network context.
3. Manual approval workflow для сетевых ограничений.
4. Controlled enforcement через allowlist политик.
5. Audit trail каждого сетевого действия.

До отдельного решения владельца продукта pfSense runtime остается no-touch.
