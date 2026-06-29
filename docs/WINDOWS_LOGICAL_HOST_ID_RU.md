# Stable logical host id для Windows/RDP контура

Дата фиксации: 2026-06-27.

Обновление 2026-06-29: восстановленный production RDP host доступен как
`192.168.100.19`, но logical host id остаётся `SHARKON2025`.

Этот документ описывает правило, которое защищает AWatch-rus/DetMir от поломки
при переименовании Windows/RDP сервера.

## Правило

В AWatch-rus есть два разных идентификатора:

| Поле | Назначение | Можно менять при rename Windows |
| --- | --- | --- |
| physical Windows name / `COMPUTERNAME` | имя ОС, локальный домен учеток, WinRM/администрирование | да |
| `awHostname` / logical host id | суффикс ActivityWatch bucket, Grafana переменные, ClickHouse workforce keys, worktime reports | нет, только через плановую миграцию |

Для DetMir production текущий stable logical host id:

```text
SHARKON2025
```

Это legacy logical id для сохранения истории bucket-ов и дашбордов. Он больше
не должен трактоваться как обязательное физическое имя Windows-сервера.

Физический IP/адрес администрирования задаётся отдельно в inventory/env:

```text
rdp-prod ansible_host=192.168.100.19
AW_MONITORED_WINDOWS_HOST=192.168.100.19
```

## Где задается

Windows deploy:

```yaml
aw_windows_logical_host_id: "SHARKON2025"
aw_windows_hostname_override: "{{ aw_windows_logical_host_id }}"
```

Файл:

```text
ansible/host_vars/rdp-prod.yml
```

Server-side reports:

```text
AW_WORKTIME_HOST=<logical_host_id>
AW_MONITORED_WINDOWS_HOSTNAME=<logical_host_id>
AW_WORKTIME_INFLUX_HOSTS=<logical_host_id>
AW_DLP_INFLUX_HOSTS=<logical_host_id>
```

Windows runtime config:

```text
C:\ProgramData\AWatch-rus\deployment-config.json
```

Ключ:

```json
{
  "awHostname": "SHARKON2025"
}
```

## Что делать при переименовании RDP сервера

1. Не менять `awHostname`, если нет отдельного плана миграции исторических
   bucket-ов, Grafana и ClickHouse.
2. Windows account domain / local logon prefix можно задать явно:

```yaml
aw_windows_domain: "<new_windows_computer_or_domain_name>"
```

Если `aw_windows_domain` пустой или оставлен как `HOST-EXAMPLE`, playbook
прочитает текущий `$env:COMPUTERNAME` через WinRM и использует его только для
Windows-учёток. Это не меняет `awHostname`.

3. Повторно применить Windows deploy после восстановления WinRM:

```bash
cd ansible
ansible-playbook -i inventory.ini deploy_aw_windows.yml --limit rdp-prod
```

4. Проверить на Windows:

```powershell
Get-Content -Raw 'C:\ProgramData\AWatch-rus\deployment-config.json' |
  ConvertFrom-Json |
  Select-Object awHostname,userTasks
```

5. Проверить ActivityWatch buckets:

```bash
AW_MONITORED_WINDOWS_HOSTNAME=SHARKON2025 ./check-aw-data.sh
```

## Что ломается, если использовать `COMPUTERNAME` как bucket id

- появляются новые пустые bucket-и после rename;
- старые dashboards продолжают смотреть на старый host;
- `aw-worktime-api` считает источники stale/missing;
- ClickHouse workforce catalog перестает связывать пользователей с событиями;
- guard/recovery может искать неправильные launch tasks.

## Миграция на новый logical id

Переход с `SHARKON2025` на нейтральный id вроде `DETMIR-RDP-01` допустим только
как отдельная planned migration:

- остановить Windows collectors;
- создать mapping старого и нового logical id;
- обновить Grafana dashboards;
- обновить ClickHouse workforce catalog;
- решить, переносить ли исторические ActivityWatch bucket-и или оставить их
  read-only;
- обновить `AW_WORKTIME_HOST`, `AW_MONITORED_WINDOWS_HOSTNAME`,
  `AW_WORKTIME_INFLUX_HOSTS`, `AW_DLP_INFLUX_HOSTS`;
- перезапустить server-side services;
- выполнить smoke-check и ручную проверку портала.

Без этих шагов менять logical id в production нельзя.
