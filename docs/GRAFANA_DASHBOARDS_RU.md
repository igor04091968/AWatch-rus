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

1. `AWatch-rus: Работа пользователей в RDP`
2. `AWatch-rus: DLP и ИБ обзор`
3. `AWatch-rus: ИБ сводка для руководства`
4. `AW-rus: DLP обзор`

По умолчанию playbook складывает их в folder `AWatch-rus` с `uid=awatch-rus`.

## Worktime panels и canonical users

Worktime dashboard'ы читают InfluxDB measurement
`aw_rdp_worktime_daily`/`aw_rdp_worktime_hourly` и группируют данные по user
label. Старые exporter versions писали raw `username`/`userId`, поэтому в
Influx могли остаться отдельные series для `USER5/user5`,
`Администратор/администратор`, machine account `SHARKON2025$` и битых строк с
Unicode replacement char `�`.

Version-controlled dashboard JSON должны сохранять защиту от старых series:

- `grafana/detmir-rdp-user-activity-dashboard.json`;
- `grafana/detmir-aw-main-dashboard.json`.

Для affected Flux queries обязательны правила:

- фильтровать `user_id !~ /\$$/` и `user_id !~ /�/`;
- мапить текущие DetMir accounts в canonical labels:
  `user1`, `user4`, `user5`, `Администратор`;
- grouping делать по `report_date,user` или `_time,user`;
- использовать `max(column: "_value")` после grouping, чтобы схлопнуть
  duplicate series без удвоения часов.

После импорта проверять панель `Вчера: активность по сотрудникам`. Ожидаемые
labels: `user1`, `user4`, `user5`, `Администратор`. Bad labels list должен быть
пустым для `USER*`, `SHARKON2025$`, `администратор`, `�` и labels, начинающихся
с `\`.

## Доступ владельца из портала

На production-контуре DetMir переход из `/portal` к Grafana dashboard'ам
выполняется без второго логина Grafana. Внешняя защита при этом остается на
gateway:

- `/portal/`, `/d/...`, `/dashboards` и `/r/grafana/` закрыты nginx Basic Auth;
- nginx после успешной gateway-авторизации передает в Grafana auth-proxy
  заголовки:
  - `X-WEBAUTH-USER: detmir-owner`;
  - `X-WEBAUTH-NAME: AWatch-rus Owner`;
  - `X-WEBAUTH-EMAIL: owner@awatch-rus.local`;
- Grafana принимает auth-proxy только от gateway `10.10.10.2`;
- созданный пользователь `detmir-owner` не является Grafana admin и получает
  viewer-доступ.

Основной dashboard для владельца:

```text
/d/detmir-rdp-user-activity/detmir3a-rabota-pol-zovatelej-v-rdp?orgId=1&from=now-7d&to=now&timezone=browser&var-host=SHARKON2025&refresh=5m
```

В портале он доступен как кнопка `Графики сотрудников`.

Не включайте `[auth.anonymous]` для решения этой задачи: это откроет Grafana на
внутреннем адресе `10.10.10.11:3000` без пользовательского контекста. Для
production используется только auth-proxy с whitelist gateway.

## Быстрый запуск

1. Подготовьте inventory и vars:

```bash
cd <PROJECT_ROOT>
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

## Production fallback при 403

Если Grafana API import запрещен (`403`) или provisioning не перезаписывает уже
существующую DB-запись dashboard, не правьте JSON только в UI. Сначала
обновите version-controlled dashboard JSON в git, затем примените один из
fallback paths.

Provisioning push:

```bash
scp grafana/detmir-aw-main-dashboard.json grafana/detmir-rdp-user-activity-dashboard.json igor@10.10.10.2:~/codex-dashboard-import/
ssh igor@10.10.10.2 'sudo pct push 201 /home/igor/codex-dashboard-import/detmir-aw-main-dashboard.json /etc/grafana/provisioning/dashboards/aw/detmir-aw-main.json --perms 0644'
ssh igor@10.10.10.2 'sudo pct push 201 /home/igor/codex-dashboard-import/detmir-rdp-user-activity-dashboard.json /etc/grafana/provisioning/dashboards/aw/detmir-rdp-user-activity.json --perms 0644'
ssh igor@10.10.10.2 'sudo pct exec 201 -- bash -lc "cp -a /var/lib/grafana/grafana.db /var/lib/grafana/grafana.db.bak.$(date -u +%Y%m%dT%H%M%SZ); systemctl restart grafana-server"'
```

DB fallback: после backup `/var/lib/grafana/grafana.db` заменить только
`dashboard.data` rows по uid нужных dashboard'ов и перезапустить
`grafana-server`. Для исправления worktime-дублей production backup был:

```text
/var/lib/grafana/grafana.db.bak.20260609T013605Z
```

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
