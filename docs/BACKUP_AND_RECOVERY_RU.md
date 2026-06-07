# Backup and Recovery

Документ описывает базовую модель резервирования и восстановления AWatch-rus.

Конкретный список файлов, баз и services должен уточняться для release profile
и инфраструктуры заказчика.

## Что резервировать

Конфигурация:

- service environment files;
- portal/backend config;
- agent config templates;
- reverse proxy config;
- access control settings;
- deployment inventory templates without secrets in Git.

Данные:

- reports;
- local state;
- telemetry state where applicable;
- evidence metadata;
- investigation/case state;
- readiness bundles;
- release manifests and checksums.

Не хранить в публичном Git:

- passwords;
- tokens;
- private inventory;
- runtime databases;
- customer evidence;
- live screenshots.

## Резервирование конфигурации

Рекомендуемый подход:

1. Хранить sanitized templates в Git.
2. Хранить secrets в защищенном хранилище заказчика.
3. Перед изменениями сохранять текущие service configs.
4. Фиксировать release commit and artifact checksums.
5. Проверять, что rollback path не зависит от ноутбука администратора.

## Резервирование отчетов

Reports and evidence metadata должны резервироваться по политике заказчика.

Минимально:

- daily backup for pilot;
- backup before upgrades;
- separate backup for release manifests;
- restore test before production acceptance.

## Восстановление

Общий порядок:

1. Остановить affected services, если это требуется recovery-планом.
2. Сохранить текущий сбойный state для анализа.
3. Восстановить config/state из backup.
4. Запустить services.
5. Проверить `/healthz`.
6. Проверить `/readyz`.
7. Проверить `/metrics`.
8. Выполнить deployment smoke.
9. Зафиксировать recovery result.

## Rollback после обновления

Перед обновлением:

- сохранить release version;
- сохранить service configs;
- сохранить checksums;
- выполнить smoke baseline;
- подготовить rollback commands.

После rollback:

- проверить portal;
- проверить API reports;
- проверить role gates;
- проверить data freshness;
- зафиксировать причину rollback.

## Ограничения

- Backup не заменяет monitoring.
- Restore должен проверяться до production acceptance.
- Evidence/customer data не должны попадать в публичный repository.
- Demo fixtures не являются production backup.
- Для юридически значимого хранения evidence требуется отдельный контур и
  отдельные требования.
