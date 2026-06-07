# Sizing Guide

Документ дает ориентиры для планирования AWatch-rus deployment.

Важно: оценки являются предварительными и требуют проверки на инфраструктуре
заказчика. Нагрузка зависит от частоты событий, периода хранения, числа
источников, объема reports/evidence и выбранных integrations.

## До 50 пользователей

Профиль:

- standalone или small pilot;
- один backend/portal host;
- локальное state/report storage;
- базовый backup.

Ориентиры:

- начать с минимальной инсталляции;
- включать только согласованные источники;
- выполнить smoke and readiness checks;
- проверить, что reports строятся без timeout.

## До 250 пользователей

Профиль:

- dedicated backend/portal host;
- отдельный storage volume;
- reverse proxy with TLS;
- регулярный backup;
- monitoring `/healthz`, `/readyz`, `/metrics`.

Ориентиры:

- валидировать retention policy;
- ограничивать тяжелые report queries;
- проверять data freshness;
- фиксировать coverage expectations.

## До 1000 пользователей

Профиль:

- отдельный backend/API host;
- выделенное storage profile;
- monitoring and alerting;
- staged rollout by departments;
- documented backup/restore;
- smoke after each rollout wave.

Ориентиры:

- проводить load validation;
- проверять report cache/fallback behavior;
- разделять pilot/demo data и production data;
- контролировать очереди/spool на agents;
- учитывать storage growth for evidence metadata.

## Более 1000 пользователей

Профиль:

- enterprise architecture review required;
- staged deployment;
- выделенный storage and backup design;
- monitoring/SLO;
- access control review;
- integration acceptance for each optional addon;
- capacity testing before production rollout.

Ориентиры:

- не переносить pilot sizing автоматически;
- проводить нагрузочные и recovery проверки;
- оценивать retention and backup windows;
- фиксировать ownership of each source;
- использовать rollback plan for rollout waves.

## Факторы нагрузки

- число endpoint hosts;
- частота telemetry events;
- период хранения;
- количество role-based report users;
- объем evidence metadata;
- наличие screenshots/evidence в конкретном контуре;
- optional integrations;
- частота smoke/readiness checks.

## Что нельзя заявлять

- гарантированную производительность без проверки;
- универсальное sizing правило для всех заказчиков;
- готовность optional integrations без отдельной приемки;
- отсутствие необходимости backup/recovery тестов.
