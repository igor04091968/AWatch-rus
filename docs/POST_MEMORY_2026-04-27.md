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

## Зафиксированные выводы

- На текущем этапе `10.10.10.2` является фактическим runtime-хостом для `AW server` и `pfSense poller`.
- Документация должна исходить из host-based сценария как из подтвержденного рабочего контура, а CT/LXC-схему держать как отдельный вариант развертывания.
