# Скрипты выполнения TЗ поддержки DetMir

В репозитории добавлены скрипты для регламентных проверок по
`docs/DETMIR_SUPPORT_TASKS_RU.md`:

- `scripts/detmir-support-run.sh --scope {daily|weekly|monthly}`
- `scripts/detmir-support-daily.sh`
- `scripts/detmir-support-weekly.sh`
- `scripts/detmir-support-monthly.sh`

## Быстрый старт

1. Скопировать шаблон окружения:

```bash
cp scripts/detmir-support.env.example scripts/detmir-support.env
```

2. Внести фактические IP/имена хостов, сервисы, пути бэкапов, VM IDs и SSH-данные.

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

## Что делает скрипт

Собираются проверки по режиму:

- **daily**: ключевая доступность, сервисы, диски, VM/CT, бэкапы, task log, Suricata.
- **weekly**: всё из `daily` + расширенные проверки логов и журналов.
- **monthly**: всё из `weekly` + проверка апдейтов и базовая фиксация документов/DR-процесса.

## Результаты

В каталоге отчётов создаются файлы:

- `support-<scope>-<timestamp>.log` — детальный лог запуска
- `support-<scope>-<timestamp>.csv` — машинный реестр проверок
- `support-<scope>-<timestamp>.md` — человекочитаемый отчёт

Коды выхода:

- `0` — без ошибок и предупреждений
- `1` — есть WARN
- `2` — есть FAIL
