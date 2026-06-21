# Runbook: backup и restore Gitea

Статус: registry-readiness runbook. Документ фиксирует целевую схему backup
для self-hosted Gitea AWatch-rus. Backup нельзя считать production-ready до
успешного тестового восстановления на отдельном сервере.

## Параметры backup-контура

| Параметр | Значение |
| --- | --- |
| Backup path | `/var/backups/gitea` |
| Backup script | `/usr/local/sbin/awatch-gitea-backup.sh` |
| systemd service | `awatch-gitea-backup.service` |
| systemd timer | `awatch-gitea-backup.timer` |
| Schedule target | daily `03:20` with `RandomizedDelaySec=10m` |
| Backup format | Gitea dump ZIP |
| Checksum | SHA256 |
| Retention | 14 days |

Целевой backup script использует `gitea dump`, создает ZIP backup и отдельный
SHA256 checksum. Каталог `/var/backups/gitea` должен быть доступен только
административным пользователям, обслуживающим Gitea backup.

## Проверка timer

```bash
systemctl status awatch-gitea-backup.timer --no-pager
systemctl list-timers awatch-gitea-backup.timer --no-pager
```

## Проверка результата backup

```bash
ls -lh /var/backups/gitea
sha256sum -c /var/backups/gitea/<backup>.zip.sha256
```

Имя backup-файла должно включать timestamp или иной однозначный идентификатор
запуска. Retention target - 14 days.

## Restore outline

Restore является ручной процедурой и требует тестового восстановления на
отдельном сервере перед признанием backup production-ready.

Общий порядок:

1. Подготовить отдельный сервер или isolated test instance.
2. Установить совместимую версию Gitea.
3. Остановить Gitea на целевом тестовом сервере.
4. Проверить SHA256 checksum выбранного ZIP backup.
5. Выполнить восстановление из `gitea dump` согласно официальной процедуре
   Gitea для используемой версии.
6. Проверить права на каталоги, repository storage, database, custom config и
   attachments.
7. Запустить Gitea.
8. Выполнить post-restore checks.
9. Зафиксировать timestamp, backup filename, checksum, Gitea version,
   restore duration и результат проверки.

## Post-restore checks

```bash
systemctl status gitea --no-pager
curl -L https://git.iri1968.dpdns.org | head
gitea doctor check
```

Если менялся путь установки или сервер переносился, выполнить:

```bash
gitea admin regenerate hooks
```

## Ограничения

- Restore-runbook не заменяет фактический restore test.
- Backup не должен считаться production-ready, пока restore не проверен на
  отдельном сервере.
- Offsite copy должна быть описана отдельно до финальной подачи в реестр.
- Секреты, токены и приватные ключи не включаются в backup evidence manifest.
