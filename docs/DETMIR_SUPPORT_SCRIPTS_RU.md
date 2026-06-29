# Скрипты выполнения TЗ поддержки DetMir

В репозитории добавлены скрипты для регламентных проверок по
`docs/DETMIR_SUPPORT_TASKS_RU.md`:

- `scripts/detmir-support-run.sh --scope {daily|weekly|monthly}`
- `scripts/detmir-support-daily.sh`
- `scripts/detmir-support-weekly.sh`
- `scripts/detmir-support-monthly.sh`

## Быстрый старт

1. Создать приватный файл окружения вне репозитория:

```bash
mkdir -p "$HOME/.config/awatch-rus"
cp scripts/detmir-support.env.example "$HOME/.config/awatch-rus/detmir-support.env"
chmod 600 "$HOME/.config/awatch-rus/detmir-support.env"
```

2. Внести фактические IP/имена хостов, сервисы, пути бэкапов, VM IDs и SSH-данные.
   Реальные пароли и приватные ключи не должны храниться в репозитории.

3. Запустить нужный режим:

```bash
./scripts/detmir-support-daily.sh
./scripts/detmir-support-weekly.sh
./scripts/detmir-support-monthly.sh
```

Также можно указать отдельный каталог отчётов:

```bash
./scripts/detmir-support-run.sh --scope daily --output-dir /var/log/detmir-support
```

## Файл окружения

Скрипт загружает параметры в таком порядке:

1. файл из переменной `DETMIR_SUPPORT_ENV_FILE`, если она задана;
2. `scripts/detmir-support.env`, если он существует;
3. `$HOME/.config/awatch-rus/detmir-support.env`, если локального файла в
   репозитории нет.

Рекомендуемый промышленный вариант — хранить секреты в
`$HOME/.config/awatch-rus/detmir-support.env`, а в репозитории держать только
`scripts/detmir-support.env.example`.

Минимальные параметры для текущего контура DetMir:

```bash
DETMIR_SUPPORT_PVE_HOST=10.10.10.2
DETMIR_SUPPORT_AW_HOST=10.10.10.13
DETMIR_SUPPORT_WEB_HOST=10.10.10.2
DETMIR_SUPPORT_WEB_TLS_HOST=10.10.10.2
DETMIR_SUPPORT_WINDOWS_HOST=192.168.100.19
DETMIR_SUPPORT_SURICATA_HOST=10.10.10.2

DETMIR_SUPPORT_SSH_USER=igor
DETMIR_SUPPORT_AW_SSH_USER=igor
DETMIR_SUPPORT_PVE_BACKUP_DIRS=/var/lib/pve/local-btrfs/dump
```

Парольные переменные вида `DETMIR_SUPPORT_AW_SSH_PASSWORD` допускаются только в
локальном приватном env-файле. В документации, Git и отчетах пароли не
фиксируются.

## Что делает скрипт

Собираются проверки по режиму:

- **daily**: ключевая доступность, сервисы, диски, VM/CT, бэкапы, task log, Suricata.
- **weekly**: всё из `daily` + расширенные проверки логов и журналов.
- **monthly**: всё из `weekly` + проверка апдейтов и базовая фиксация документов/DR-процесса.

Актуальные особенности текущего контура:

- операторский gateway находится на `10.10.10.2`, а не на историческом
  `10.10.10.11`;
- текущий Windows/RDP target после восстановления: `192.168.100.19`;
- web health endpoint: `https://10.10.10.2/healthz`, ожидаемый ответ `200`;
- защищенный корень gateway `https://10.10.10.2/` штатно отвечает `401`;
- AW API health проверяется через
  `http://10.10.10.13:5600/api/0/settings/`;
- актуальный каталог Proxmox backup storage:
  `/var/lib/pve/local-btrfs/dump`;
- если `suricata.service` не активен, проверка процесса Suricata
  пропускается как следствие состояния сервиса, а не как отдельный сбой
  процесса.

## Результаты

В каталоге отчётов создаются файлы:

- `support-<scope>-<timestamp>.log` — детальный лог запуска
- `support-<scope>-<timestamp>.csv` — машинный реестр проверок
- `support-<scope>-<timestamp>.md` — человекочитаемый отчёт

Коды выхода:

- `0` — без ошибок и предупреждений
- `1` — есть WARN
- `2` — есть FAIL

`SKIP` означает, что проверка не могла быть выполнена в текущих условиях:
например, нет SSH-аутентификации, сервис намеренно выключен или отсутствует
проверяемый компонент. `SKIP` не должен маскировать причину: в строке отчета
должна быть указана диагностическая причина, например
`Permission denied (publickey,password)` или
`Skipped because suricata.service state=inactive`.

## Текущие ожидаемые предупреждения

На момент актуализации документации для контура DetMir допустимо увидеть:

- `aw-server-rust` на `10.10.10.13` в состоянии `inactive`, если фактический
  production service — `activitywatch-server`;
- предупреждение по резервным копиям, если последний файл в
  `/var/lib/pve/local-btrfs/dump` старше установленного порога;
- `suricata.service state=inactive`, если IDS/IPS на данном узле не введен в
  штатную эксплуатацию.

Эти предупреждения нужно фиксировать в отчете и отдельно согласовывать:
включать сервис, менять список ожидаемых сервисов или отмечать компонент как
неиспользуемый.
