# Linux remote worker deployment

## Назначение

Этот сценарий закрывает полный набор данных по Linux-удалёнщику:

- GUI active window и `afk` через `ActivityWatch`;
- SSH и shell-команды через console/ssh logger;
- браузерные админки по title-based правилам, включая Proxmox `https://...:8006`.

Итоговый целевой набор bucket'ов:

- `aw-watcher-window_<HOST>`
- `aw-watcher-afk_<HOST>`
- `aw-console-commands_<HOST>`
- `aw-ssh-sessions_<HOST>`
- `aw-linux-web-context_<HOST>`
- `aw-detmir-web-category_<HOST>`

## Установка

```bash
cd /path/to/AWatch-rus
sh ./scripts/install_aw_linux_remote_worker.sh \
  --server-host 10.10.10.13 \
  --server-port 5600
```

## Что ставится

1. `scripts/install_aw_linux_client.sh`
2. `scripts/install_aw_console_ssh_logger.sh`
3. `scripts/install_aw_linux_web_category_logger.sh`

## Что даёт web-category logger

Это отдельный user-space collector, который смотрит активное окно в X11 и по title/class
пытается классифицировать браузерные рабочие интерфейсы.

Из коробки есть правила для:

- Proxmox Web UI
- pfSense Web UI
- Grafana

Правила лежат в:

```bash
~/.config/aw-linux-web-category/rules.json
```

Для Proxmox `:8006` collector пишет события в `aw-detmir-web-category_<HOST>` с полями вроде:

- `categoryGroup=work`
- `category=Администрирование`
- `service=proxmox`
- `interface=https`
- `port=8006`
- `rootDomain=proxmox-webui`

## Проверка

На клиенте:

```bash
~/.local/bin/aw-console-ssh-logger-status
~/.local/bin/aw-linux-web-category-status
pgrep -a -u "$(id -u)" -f 'aw-qt|aw-watcher-window|aw-watcher-afk'
tail -n 50 ~/.local/state/aw-console-ssh-logger/logs/collector.log
tail -n 50 ~/.local/state/aw-linux-web-category/logs/collector.log
```

На AW server:

```bash
curl -fsS http://10.10.10.13:5600/api/0/buckets | jq -r 'keys[]' | \
  grep -E '^aw-watcher-window_|^aw-watcher-afk_|^aw-console-commands_|^aw-ssh-sessions_|^aw-linux-web-context_|^aw-detmir-web-category_'
```

## Ограничения

- `aw-watcher-window` и `aw-watcher-afk` требуют реальную desktop-сессию.
- Web-category logger опирается на X11 active window title и `WM_CLASS`.
- Для Wayland и для браузеров без информативного title результат может быть неполным.
- Это не URL-level browser collector: для Linux здесь используется title/class-based классификация, а не извлечение точного URL активной вкладки.
