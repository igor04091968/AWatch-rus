# Preparation

## Цель

Подготовить чистое окружение для повторяемого развёртывания отдельного `ActivityWatch Server` в LXC-контейнере на Proxmox без зависимости от старой инсталляции.

## Что определить заранее

### Инфраструктурные параметры

- имя узла Proxmox;
- `CT_ID`;
- `CT_HOSTNAME`;
- `CT_BRIDGE`;
- `CT_VLAN`, если нужен;
- статический IP/маска/шлюз;
- список DNS;
- storage для `rootfs`;
- размер диска;
- лимиты CPU/RAM/SWAP;
- timezone;
- внутренний или внешний FQDN для публикации.

### Доступы

Хранить вне репозитория:

- SSH/API-доступ к Proxmox;
- пароль или ключ для CT;
- TLS-сертификаты reverse proxy;
- токены внешнего мониторинга или backup target.

## Рекомендуемая базовая схема

- Proxmox VE 8/9
- Debian 12 LXC
- `unprivileged=1`
- `nesting=1,keyctl=1`
- внутренний listen ActivityWatch: `0.0.0.0:5600`
- внешняя публикация только через VPN или reverse proxy

## Проверки на узле Proxmox

```sh
pveversion
pveam update
pveam available | grep debian-12
pvesm status
pct list
```

Проверить:

- есть место под CT и backup;
- `CT_ID` не занят;
- выбранный bridge/VLAN реально маршрутизируется;
- Debian template доступен;
- есть маршрут до будущего адреса контейнера.

## Что подготовить до деплоя

- заполненный `secrets/deploy.secrets.env` (единый файл для CT + AW server);
- согласованный URL релиза `aw-server-rust`;
- решение по публикации: VPN или reverse proxy;
- решение по backup: `vzdump`, snapshot, rsync, NAS или object storage.

## Единый файл секретов

Файл `secrets/deploy.secrets.env` автоматически подхватывается:

- `proxmox/create-ct.sh`
- `proxmox/push-aw-artifacts.sh`

В нём хранятся:

- `CT_*` параметры контейнера;
- `AW_SERVER_*` параметры сервера.

## Базовые каталоги внутри CT

- `/opt/activitywatch/bin`
- `/opt/activitywatch/releases`
- `/opt/activitywatch/webui-ru`
- `/etc/activitywatch`
- `/var/lib/activitywatch`
- `/var/log/activitywatch`

## Security-заметки

- не публиковать `5600/tcp` напрямую в интернет;
- не хранить реальные значения в `.example` файлах;
- перед вводом в эксплуатацию зафиксировать firewall, DNS и publish path в `docs/operations.md`.
