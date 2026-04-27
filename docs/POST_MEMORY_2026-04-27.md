# POST MEMORY — AWatch-rus (2026-04-27)

## Контекст дня

Работа велась по проекту `AWatch-rus / ActivityWatch-Russian`.

## Что подтверждено по runtime

- `10.10.10.2` (`pve-detmir`, admin host) реально запускает:
  - `/usr/local/bin/aw-server-rust --host 0.0.0.0 --port 5600 --webpath /opt/aw-webui-ru`
  - `/usr/bin/python3 /opt/aw-pfsense/pfsense-aw-poller.py --config /etc/aw-pfsense/poller.json`
- `10.10.10.1` используется только как API-цель для `pfSense poller`.
- Значит интеграция `pfSense -> AW` работает через внешний poller, а не через установку чего-либо на сам firewall.

## Что сделано

1. Восстановлена и синхронизирована локальная копия проекта.
- В `/mnt/usb_hdd2/Projects/ActivityWatch-Russian` был поврежден `.git`.
- Источник истины: `/home/igor/tmp/AWatch-rus` с `origin https://github.com/igor04091968/AWatch-rus.git`.
- Локальная копия приведена к актуальному содержимому `main`.

2. Завершена русификация overview-дашборда Grafana.
- `1C Бухгалтерия — Overview` заменено на `1C Бухгалтерия — Обзор`.
- Документация синхронизирована с фактическим названием.
- Проверка `rg -n "Overview"` по репозиторию после правки не дала совпадений.

3. Проверен локальный прокси-контур.
- `socks5://127.0.0.1:1080` активен.
- `sing-box` запущен и пропускает трафик.

4. Подготовлен и развернут Linux-клиент ActivityWatch для `10.10.10.2`.
- В репозиторий добавлен rollout:
  - `scripts/install_aw_linux_client.sh`
  - `docs/linux-client.md`
- На `10.10.10.2` под пользователем `admin` установлен bundle `ActivityWatch 0.13.2` в:
  - `~/.local/opt/activitywatch/v0.13.2/activitywatch`
- Созданы:
  - `~/.config/activitywatch/aw-client/aw-client.toml` с `10.10.10.13:5600`
  - `~/.config/activitywatch/aw-qt/aw-qt.toml`
  - `~/.local/bin/activitywatch-remote-aw`
  - `~/.config/autostart/activitywatch-remote-aw.desktop`
- Проверка показала:
  - удаленный `AW server` с `10.10.10.2` достижим;
  - `aw-qt --no-gui` стартует;
  - bucket `aw-watcher-afk_pve-detmir` появился на сервере.
- Текущий blocker:
  - на `10.10.10.2` нет активной `X11/GUI` сессии;
  - `aw-watcher-window` падает с `DISPLAY environment variable not set`;
  - `aw-watcher-afk` падает на `failed to acquire X connection`.

## Зафиксированные выводы

- На текущем этапе `10.10.10.2` является фактическим runtime-хостом для `AW server` и `pfSense poller`.
- Документация должна исходить из host-based сценария как из подтвержденного рабочего контура, а CT/LXC-схему держать как отдельный вариант развертывания.
- Для Linux-клиента на `10.10.10.2` установка завершена, но полноценные watcher-события начнутся только после реального графического логина пользователя.
