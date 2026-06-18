# Полная диагностика развернутого контура DetMir

Пакет предназначен для расширенной проверки уже развернутого контура
DetMir, не заменяет и не дублирует скрипты TЗ поддержки.

Состав:
- `detmir-full-diagnostics.sh` — оркестратор:
  - `aw-contour-diag.sh` (по-умолчанию полный прогон),
  - `detmir-support-run.sh` (daily/weekly/monthly),
  - `check_production_inventory_placeholders.sh`.
- `detmir-full-diagnostics-daily.sh` — быстрый старт с `--scope daily`.
- `detmir-full-diagnostics-weekly.sh` — запуск с `--scope weekly`.
- `detmir-full-diagnostics-monthly.sh` — запуск с `--scope monthly`.
- `detmir-full-diagnostics.env.example` — шаблон параметров.

## Быстрый старт

1. Скопировать файл конфигурации:

```bash
cp scripts/detmir-full-diagnostics/detmir-full-diagnostics.env.example \
   scripts/detmir-full-diagnostics/detmir-full-diagnostics.env
```

2. Заполнить пути/хосты под текущий контур.

3. Запуск:

```bash
./scripts/detmir-full-diagnostics/detmir-full-diagnostics-daily.sh
./scripts/detmir-full-diagnostics/detmir-full-diagnostics.sh --scope weekly --quick
./scripts/detmir-full-diagnostics/detmir-full-diagnostics-monthly.sh --output-dir /var/log/detmir-full-diagnostics
```

## Примечание

- Для запуска в `~/root/scripts/support` пакет может использовать скопированные
  `aw-contour-diag.sh` и `check_production_inventory_placeholders.sh`.
  Если локально эти файлы не требуются — удалите их или отключите шаги флагами.
- Для запуска в окружении с root-доступом через `sudo` выполняйте скрипты как
  пользователь с правом `sudo` без `NOPASSWD`.
