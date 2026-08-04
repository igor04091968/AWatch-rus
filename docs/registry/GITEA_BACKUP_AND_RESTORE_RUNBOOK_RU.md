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
| Restore tested | `false` until a separate test restore is completed |

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
2. Установить ту же версию Gitea, из которой был создан dump.
3. Остановить Gitea на целевом тестовом сервере.
4. Проверить SHA256 checksum выбранного ZIP backup.
5. Распаковать dump.
6. Восстановить `app.ini`, data, repositories и database согласно официальной
   restore-процедуре Gitea для используемой версии.
7. Исправить ownership и permissions:

```bash
chown -R git:git /var/lib/gitea
chown root:git /etc/gitea/app.ini
chmod 640 /etc/gitea/app.ini
```

8. Запустить Gitea.
9. Выполнить `gitea doctor check`.
10. Если менялся путь установки или переносился сервер, выполнить
    `gitea admin regenerate hooks`.
11. Выполнить post-restore checks.
12. Зафиксировать timestamp, backup filename, checksum, Gitea version,
   restore duration и результат проверки.

## Post-restore checks

```bash
systemctl status gitea --no-pager
curl -L https://git.iri1968.ru | head
gitea doctor check
```

Также открыть в браузере:

```text
https://git.iri1968.ru/awatch-rus/AWatch-rus
```

Если менялся путь установки или сервер переносился, выполнить:

```bash
gitea admin regenerate hooks
```

## Ограничения

- Restore-runbook не заменяет фактический restore test.
- `restore_tested=false`, пока не выполнено тестовое восстановление на
  отдельном сервере.
- Backup не должен считаться production-ready, пока restore не проверен на
  отдельном сервере.
- Offsite copy должна быть описана отдельно до финальной подачи в реестр.
- Секреты, токены и приватные ключи не включаются в backup evidence manifest.
