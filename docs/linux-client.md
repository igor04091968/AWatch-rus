# Linux client deployment

## Назначение

Этот сценарий ставит `ActivityWatch` в user-space Linux-хоста и направляет watcher'ы на уже существующий `AW server`, не поднимая локальный `aw-server-rust`.

Подтвержденный целевой кейс:

- Linux desktop/admin host: `10.10.10.2`
- удаленный `AW server`: `10.10.10.13:5600`

## Что делает скрипт

- скачивает официальный Linux ZIP `ActivityWatch`;
- распаковывает bundle в `~/.local/opt/activitywatch/v<version>`;
- обновляет symlink `~/.local/opt/activitywatch/current`;
- пишет `aw-client.toml` с указанием удаленного `AW server`;
- пишет `aw-qt.toml`, где оставляет только `aw-watcher-afk` и `aw-watcher-window`;
- создает launcher `~/.local/bin/activitywatch-remote-aw`;
- создает XDG autostart entry `~/.config/autostart/activitywatch-remote-aw.desktop`.

## Установка

```bash
cd /path/to/AWatch-rus
sh ./scripts/install_aw_linux_client.sh \
  --server-host 10.10.10.13 \
  --server-port 5600
```

Если нужно переустановить ту же версию:

```bash
sh ./scripts/install_aw_linux_client.sh --force
```

## Ключевые файлы

- `~/.config/activitywatch/aw-client/aw-client.toml`
- `~/.config/activitywatch/aw-qt/aw-qt.toml`
- `~/.local/bin/activitywatch-remote-aw`
- `~/.config/autostart/activitywatch-remote-aw.desktop`

## Ручной запуск

```bash
~/.local/bin/activitywatch-remote-aw
```

## Проверка

Проверка bundle и конфигурации:

```bash
~/.local/opt/activitywatch/current/aw-qt --help
~/.local/opt/activitywatch/current/aw-watcher-afk/aw-watcher-afk --help
~/.local/opt/activitywatch/current/aw-watcher-window/aw-watcher-window --help
cat ~/.config/activitywatch/aw-client/aw-client.toml
cat ~/.config/activitywatch/aw-qt/aw-qt.toml
```

Проверка на сервере:

```bash
curl -fsS http://10.10.10.13:5600/api/0/buckets | jq -r 'keys[]' | grep '^aw-watcher-'
```

## Ограничение

`aw-watcher-window` и `aw-watcher-afk` требуют реальную пользовательскую desktop-сессию (`X11`/поддерживаемый GUI login). Если на хосте нет активного GUI-сеанса, установка пройдет, но событий от watcher'ов не будет до первого нормального графического входа пользователя.
