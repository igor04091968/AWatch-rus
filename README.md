# ActivityWatch-Russian

Практический каркас проекта для повторного развёртывания ActivityWatch Server в новом окружении с LXC-контейнером на Proxmox, русифицированным Web UI, systemd-юнитами, шаблонными скриптами деплоя и эксплуатационной документацией.

## Что входит

- `docs/preparation.md` — подготовка инфраструктуры и входных параметров.
- `docs/deployment.md` — пошаговый деплой LXC и ActivityWatch Server.
- `docs/runbook.md` — быстрый runbook для оператора.
- `docs/operations.md` — регламент сопровождения, бэкапов, обновлений и rollback.
- `proxmox/` — шаблонные скрипты подготовки и наполнения CT на стороне Proxmox.
- `aw-server/` — установочные скрипты, env-шаблон, systemd unit и RU patch для Web UI.

## Базовый сценарий

1. Подготовить параметры окружения по `docs/preparation.md`.
2. Заполнить единый файл секретов `secrets/deploy.secrets.env` (автоподключение).
3. На узле Proxmox создать контейнер через `proxmox/create-ct.sh`.
4. Загрузить артефакты и серверный env в CT через `proxmox/push-aw-artifacts.sh`.
5. Внутри контейнера выполнить `aw-server/install_aw_server.sh`.
6. Применить русификацию Web UI через `aw-server/apply_webui_ru_patch.sh`.
7. Проверить API, Web UI и состояние systemd по `docs/runbook.md`.

Скрипты `proxmox/create-ct.sh` и `proxmox/push-aw-artifacts.sh` по умолчанию читают:

- `secrets/deploy.secrets.env`

## Принципы

- Никаких реальных секретов, токенов и боевых IP в репозитории.
- Все переменные вынесены в `.example` / `.env` шаблоны.
- Документация ориентирована на повторяемое развёртывание, а не на одноразовую ручную установку.
- Rollback и backup описаны как обязательная часть каждой операции.

## Минимальная структура

- CT/LXC на Debian 12
- ActivityWatch Server Rust release
- Web UI override в `/opt/activitywatch/webui-ru`
- systemd unit `activitywatch-server.service`
- bind/listen через переменные окружения

## Ограничения

- Windows-клиенты и watcher-автоматизация в этой части каркаса не описываются.
- Интеграции с InfluxDB/Grafana/LDAP оставлены как следующий слой, не как обязательная база.

## Быстрые ссылки

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/docs/FULL_DEPLOYMENT_MANUAL_RU.md`
- `docs/preparation.md`
- `docs/deployment.md`
- `docs/runbook.md`
- `docs/operations.md`
- `proxmox/create-ct.sh`
- `aw-server/install_aw_server.sh`
