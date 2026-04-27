# Console/SSH logger deployment

## Назначение

Этот сценарий логирует только консольные действия и SSH-сессии:

- команды из `bash_history` (`aw-console-commands_<HOST>`);
- логины/логауты SSH TTY по `who -u` (`aw-ssh-sessions_<HOST>`, best-effort).

GUI watcher'ы (`aw-watcher-window`, `aw-watcher-afk`) не требуются.

## Установка (user-space)

```bash
cd /path/to/AWatch-rus
sh ./scripts/install_aw_console_ssh_logger.sh \
  --server-host 10.10.10.13 \
  --server-port 5600
```

## Что создается

- `~/.local/opt/aw-console-ssh-logger/collector.py`
- `~/.local/opt/aw-console-ssh-logger/config.json`
- `~/.local/bin/aw-console-ssh-logger-start`
- `~/.local/bin/aw-console-ssh-logger-stop`
- `~/.local/bin/aw-console-ssh-logger-status`
- `~/.config/systemd/user/aw-console-ssh-logger.service`
- `~/.local/state/aw-console-ssh-logger/` (state + logs)

## Shell hooks

Installer добавляет в `~/.bashrc`:

- `HISTTIMEFORMAT="%s "` для epoch timestamp;
- `histappend`;
- `PROMPT_COMMAND` с `history -a; history -n;`.

Это нужно, чтобы команды попадали в `~/.bash_history` сразу после выполнения, а не только при logout.

## Проверка

На клиенте:

```bash
~/.local/bin/aw-console-ssh-logger-status
tail -n 50 ~/.local/state/aw-console-ssh-logger/logs/collector.log
```

На AW сервере:

```bash
curl -fsS http://10.10.10.13:5600/api/0/buckets | jq -r 'keys[]' | grep -E '^aw-console-commands_|^aw-ssh-sessions_'
```

## Ограничения

- Источник команд — `bash_history`; если пользователь использует не `bash`, нужен отдельный collector.
- Канал `aw-ssh-sessions_*` зависит от корректного `utmp/who` на хосте и может быть неполным в отдельных окружениях.
- Полные системные audit-события (`execve` всех пользователей) требуют root-level `auditd`/journald integration и в этот user-space сценарий не входят.
