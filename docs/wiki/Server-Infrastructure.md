# Server Infrastructure

## 2.2 Local state retention

Коммит `cc9e4a0` добавил отдельный server-side maintenance path для контроля роста локального state на AW server.

## `aw-prune-local-state.sh`

Команда `aw-server/aw-prune-local-state.sh` устанавливается в:

```text
/usr/local/bin/aw-prune-local-state.sh
```

Сейчас это Rust-first wrapper. Если доступен
`/usr/local/bin/aw-prune-local-state-rust`, wrapper использует его; иначе
fallback идет на legacy shell script из
`/opt/activitywatch/aw-rus-ops/aw-prune-local-state.sh`.

Назначение:

- чистит старые backup-файлы в `{{ aw_server_data_dir }}/backups`;
- отдельно удерживает последние DB backups и JSON backups;
- удаляет временные архивы из `/tmp`: `activitywatch-*.zip`, `hayabusa-*.zip`, `aw-hayabusa-profiles.txt`;
- удаляет временные WebUI/worktime artifacts старше одного дня: `aw-worktime-ui-bridge.py`, `views-default.json`, `apply_webui_ru_patch.out`.
- чистит старые `browser-smoke` run-директории в
  `{{ aw_server_data_dir }}/browser-smoke`, удерживая последние запуски.

Rust binary по умолчанию работает в dry-run режиме. Production wrapper без
аргументов запускает его с `--apply`, чтобы сохранить поведение systemd timer.

Safety policy:

- не удалять основную ActivityWatch SQLite DB;
- не удалять rollback-critical backups: `switch-backups`, `before-rust`,
  `rollback`;
- не удалять корневые state-каталоги;
- удалять только пути из явного allowlist: `backups`, `backups/db`,
  `browser-smoke`, ограниченный набор временных файлов в `/tmp`.

## systemd unit и timer

Ansible создает:

```text
/etc/systemd/system/aw-prune-local-state.service
/etc/systemd/system/aw-prune-local-state.timer
```

Timer:

```ini
[Timer]
OnCalendar=*-*-* 04:40:00
Persistent=true
```

То есть очистка запускается ежедневно в `04:40`; если хост был выключен, `Persistent=true` догонит пропущенный запуск.

## Retention-параметры

Параметры задаются в `ansible/group_vars/aw_server.yml`:

| Переменная | Значение по умолчанию | Назначение |
| --- | ---: | --- |
| `aw_server_backup_retention_days` | `7` | Возраст backup-файлов, после которого они могут быть удалены. |
| `aw_server_backup_keep_last_db` | `2` | Минимум последних DB backup'ов, которые всегда сохраняются. |
| `aw_server_backup_keep_last_json` | `2` | Минимум последних JSON backup'ов, которые всегда сохраняются. |
| `aw_server_journal_system_max_use` | `100M` | Лимит persistent journald storage. |
| `aw_server_journal_runtime_max_use` | `50M` | Лимит runtime journald storage. |
| `aw_server_journal_system_keep_free` | `500M` | Минимально свободное место, которое journald должен оставить на FS. |

## journald retention

`deploy_aw_server.yml` устанавливает drop-in:

```text
/etc/systemd/journald.conf.d/aw-rus-retention.conf
```

Содержимое управляется Ansible:

```ini
[Journal]
SystemMaxUse={{ aw_server_journal_system_max_use }}
RuntimeMaxUse={{ aw_server_journal_runtime_max_use }}
SystemKeepFree={{ aw_server_journal_system_keep_free }}
```

После изменения drop-in playbook перезапускает `systemd-journald` и выполняет `journalctl --vacuum-size={{ aw_server_journal_system_max_use }}`. Это ограничивает рост логов без ручной очистки и снижает риск заполнения rootfs на AW server.
