# Grafana dashboard'ы AW-rus

Документ описывает воспроизводимый импорт Grafana dashboard'ов из репозитория через Ansible и HTTP API Grafana.

## Что лежит в git

Version-controlled dashboard JSON находятся в каталоге `grafana/`:

- `grafana/detmir-rdp-user-activity-dashboard.json`
- `grafana/detmir-dlp-security-dashboard.json`
- `grafana/detmir-dlp-management-dashboard.json`
- `grafana/dlp-dashboard.json`

Их импортирует playbook:

- `ansible/deploy_grafana_dashboards.yml`

## Что импортируется

1. `DetMir: Работа пользователей в RDP`
2. `DetMir: DLP и ИБ обзор`
3. `DetMir: ИБ сводка для руководства`
4. `AW-rus: DLP обзор`

По умолчанию playbook складывает их в folder `AWatch-rus` с `uid=awatch-rus`.

## Быстрый запуск

1. Подготовьте inventory и vars:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
cp ansible/inventory.example.ini ansible/inventory.ini
cp ansible/group_vars/grafana.example.yml ansible/group_vars/grafana.yml
```

2. Укажите `grafana_url` и заполните группу `[grafana]` в `ansible/inventory.ini`.

3. Перед запуском задайте пароль Grafana через переменную окружения:

```bash
export GRAFANA_ADMIN_PASSWORD='...'
```

4. Запустите импорт:

```bash
cd ansible
ansible-playbook -i inventory.ini deploy_grafana_dashboards.yml
```

## Что делает playbook

- проверяет `GET /api/health`;
- создает или обновляет folder `AWatch-rus`;
- импортирует dashboard JSON из репозитория;
- перезаписывает существующие dashboard'ы при `overwrite=true`;
- верифицирует каждый dashboard по `uid` через `GET /api/dashboards/uid/<uid>`.

## Переменные

- `grafana_url` — base URL Grafana, например `http://10.20.30.11:3000`
- `grafana_admin_user` — Grafana admin/API user
- `grafana_admin_password` — пароль, рекомендуется через `GRAFANA_ADMIN_PASSWORD`
- `grafana_validate_tls` — включать ли проверку TLS-сертификата
- `grafana_folder_uid` — UID целевого folder
- `grafana_folder_title` — отображаемое имя folder
- `grafana_dashboards_import_overwrite` — перезаписывать ли dashboard'ы при повторном импорте

## Рекомендуемый эксплуатационный режим

- редактировать dashboard JSON в `grafana/`, а не править production только руками в UI;
- после изменений прогонять `deploy_grafana_dashboards.yml`, чтобы Grafana вернулась к version-controlled состоянию;
- для презентации использовать [docs/PRESENTATION_RU.md](PRESENTATION_RU.md), где уже лежат скриншоты ключевых экранов.
