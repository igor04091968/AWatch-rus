# AWatch-rus

Практический каркас проекта для повторного развёртывания ActivityWatch Server в новом окружении с LXC-контейнером на Proxmox, русифицированным Web UI, systemd-юнитами, шаблонными скриптами деплоя и эксплуатационной документацией.

## Что входит

- `docs/preparation.md` — подготовка инфраструктуры и входных параметров.
- `docs/deployment.md` — пошаговый деплой LXC и ActivityWatch Server.
- `docs/runbook.md` — быстрый runbook для оператора.
- `docs/operations.md` — регламент сопровождения, бэкапов, обновлений и rollback.
- `docs/windows/ensemble.md` — orchestration-пакет для Windows-деплоя и проверки.
- `docs/dlp-gap-analysis.md` — разрыв до enterprise DLP и roadmap.
- `proxmox/` — шаблонные скрипты подготовки и наполнения CT на стороне Proxmox.
- `aw-server/` — установочные скрипты, env-шаблон, systemd unit и RU patch для Web UI.
- `ansible/` — Ansible-ensemble для автоматизированного сервера (Debian/CT).
- `pfsense/` — внешний poller для pfSense API и systemd unit под Debian/Ubuntu utility VM.
- `windows/` — PowerShell toolkit: single-user, domain-users, ensemble orchestration, hardening/recovery, validation, phase-2 DLP telemetry (`aw-dlp-incidents_*`, `aw-dlp-endpoint-signals_*`).
- `scripts/quality-gate.sh` — локальный preflight-пайплайн проверок.

## Базовый сценарий

1. Подготовить параметры окружения по `docs/preparation.md`.
2. Заполнить единый файл секретов `secrets/deploy.secrets.env` (автоподключение).
3. На узле Proxmox создать контейнер через `proxmox/create-ct.sh`.
4. Загрузить артефакты и серверный env в CT через `proxmox/push-aw-artifacts.sh`.
5. Внутри контейнера выполнить `aw-server/install_aw_server.sh`.
6. Применить русификацию Web UI через `aw-server/apply_webui_ru_patch.sh`.
7. Проверить API, Web UI и состояние systemd по `docs/runbook.md`.
8. Развернуть Windows-клиентов через `windows/deploy-ensemble.ps1`.
9. Проверить итог через `windows/validate-deployment.ps1`.

Для полного Ansible-сценария “с нуля” в Proxmox используйте:

- `ansible/provision_proxmox_ct_and_deploy_aw.yml`
- `ansible/provision_proxmox_ct_matrix_and_deploy_aw.yml` (массово по матрице CT)

Для централизованного phase-2 деплоя Windows-клиентов через WinRM:

- `ansible/deploy_aw_windows_phase2.yml`

Для внешнего pfSense poller'а:

- `ansible/deploy_aw_pfsense_poller.yml`

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

- Интеграции с InfluxDB/Grafana/LDAP оставлены как следующий слой, не как обязательная база.

## Быстрые ссылки

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/docs/FULL_DEPLOYMENT_MANUAL_RU.md`
- `/home/igor/tmp/AWatch-rus/docs/windows/ensemble.md`
- `docs/preparation.md`
- `docs/deployment.md`
- `docs/runbook.md`
- `docs/operations.md`
- `proxmox/create-ct.sh`
- `aw-server/install_aw_server.sh`
- `windows/deploy-ensemble.ps1`
- `windows/validate-deployment.ps1`
