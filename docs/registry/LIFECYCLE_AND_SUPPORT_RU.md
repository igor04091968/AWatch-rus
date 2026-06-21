# Lifecycle and support

Статус: registry-readiness document. Документ описывает целевой жизненный цикл
исходного кода, backup и evidence collection для AWatch-rus.

## Source code lifecycle

Целевой lifecycle исходного кода для registry-readiness проходит через
self-hosted Gitea:

```text
https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus
```

Роль Gitea: target primary Russian Git contour for registry-readiness.
Роль GitHub: public mirror only / external public repository.

Перед release или registry evidence package фиксируются:

- commit hash;
- tag, если применимо;
- список remotes;
- источник сборки;
- ответственный за выпуск;
- timestamp evidence.

## Backup lifecycle

Backup lifecycle для Gitea:

- tool: `gitea dump`;
- path: `/var/backups/gitea`;
- format: ZIP;
- checksum: SHA256;
- retention target: 14 days;
- timer: `awatch-gitea-backup.timer`;
- schedule target: daily `03:20` with `RandomizedDelaySec=10m`.

Backup нельзя считать production-ready без тестового восстановления на
отдельном сервере. Offsite copy должна быть описана отдельно.

## Restore responsibility

Restore Gitea является ручной административной процедурой. Ответственный за
restore должен:

- выбрать backup ZIP;
- проверить SHA256 checksum;
- выполнить restore на отдельном test server до использования процедуры в
  production;
- выполнить post-restore checks;
- зафиксировать результат и timestamp.

## Evidence collection before releases

Перед выпуском или передачей registry-readiness пакета собрать:

- Gitea HTTPS evidence;
- service status evidence;
- firewall evidence по внешнему `3000/tcp`;
- backup ZIP + SHA256 evidence;
- состояние `awatch-gitea-backup.timer`;
- сведения о правах доступа;
- ссылку на restore-runbook и результат restore test, если он уже выполнен.
