# Deployment Topologies

Документ описывает типовые варианты размещения AWatch-rus.

Все схемы являются ориентировочными. Реальное внедрение должно учитывать
сетевую сегментацию, политики безопасности, объем telemetry и требования к
резервному копированию.

## Standalone

Назначение: локальная экспертиза или demo.

Компоненты:

- один Linux host;
- backend/portal;
- demo fixtures;
- local smoke tooling.

Размещение:

```text
Admin workstation -> local or lab host -> AWatch-rus portal
```

Потоки данных:

- browser -> portal;
- smoke script -> health/readiness/API endpoints;
- demo fixtures -> reports/screenshots.

## Pilot

Назначение: ограниченный показ на выделенном контуре.

Компоненты:

- backend/portal host;
- ограниченная группа endpoint hosts;
- Rust Agent baseline или уже принятые источники;
- reports and readiness checks.

Размещение:

```text
Pilot endpoints -> AWatch-rus backend -> role-based portal
                                |
                                +-> reports / evidence materials
```

Потоки данных:

- endpoints -> backend telemetry path;
- backend -> reports;
- browser -> role-based portal;
- operator -> smoke and readiness checks.

## Small Company

Ориентир: до 50 пользователей.

Компоненты:

- один backend/portal host;
- локальное state/report storage;
- reverse proxy with TLS;
- basic backup.

Размещение:

```text
Endpoint group -> backend/portal host -> browser clients
```

Потоки данных:

- endpoint telemetry -> backend;
- backend -> local storage;
- portal -> role views;
- backup job -> backup storage.

## Medium Company

Ориентир: до 250 пользователей.

Компоненты:

- backend/portal host;
- separate storage or dedicated volume;
- reverse proxy;
- monitoring of health/readiness/metrics;
- scheduled backup;
- optional analytics storage where configured.

Размещение:

```text
Endpoint groups -> backend/API host -> storage
                                  -> portal/reports
                                  -> monitoring
```

Потоки данных:

- endpoint telemetry -> backend/API;
- backend -> storage and report layer;
- monitoring -> `/healthz`, `/readyz`, `/metrics`;
- operator -> smoke scripts and runbooks.

## Enterprise

Ориентир: более 250 пользователей, несколько подразделений или строгие
эксплуатационные требования.

Компоненты:

- выделенный backend/API host;
- portal/reverse proxy layer;
- dedicated storage and backup profile;
- monitoring and log collection;
- access control;
- documented rollback;
- optional integrations only after acceptance.

Размещение:

```text
Endpoint segments -> backend/API -> storage/reporting
                                -> portal via reverse proxy
                                -> monitoring/logging
                                -> backup target
```

Потоки данных:

- telemetry -> backend/API;
- backend/API -> storage/reporting;
- portal users -> reverse proxy -> portal;
- monitoring -> health/readiness/metrics;
- backup -> backup target.

## Общие правила

- Не размещать secrets в repository.
- Не публиковать portal без TLS и access control.
- Не включать optional addon как production source без acceptance.
- Не использовать demo fixtures как production data.
- Не делать sizing claims без load validation.
