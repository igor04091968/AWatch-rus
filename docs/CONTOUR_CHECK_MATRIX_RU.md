# Матрица проверки контура AWatch-rus

Дата фиксации: 2026-06-21.

Документ описывает ежедневную и еженедельную проверку эксплуатационного контура
AWatch-rus. Он не является заявлением о сертификации, не подменяет юридическую
экспертизу и не описывает GitHub Actions как основной контур выпуска.

## Цель

Ежедневная проверка должна отвечать на один вопрос: можно ли оператору считать
контур AWatch-rus пригодным для работы сегодня без ручного обхода всех
компонентов.

Еженедельная проверка расширяет ежедневную: добавляет portal smoke, registry
docs check и доказательные артефакты для последующего release/readiness пакета.

## Канонический запуск

Основной запускной скрипт:

```bash
scripts/run_awatch_contour_check.sh
```

## Локальный запуск с ноутбука

Для ручной проверки на ноутбуке удобно явно указать env-файл и корень артефактов:

```bash
export CONTOUR_CHECK_ENV_FILE=~/path/to/your/contour-check.env
export CONTOUR_CHECK_OUTPUT_ROOT=~/tmp/contour-check-runs
export CONTOUR_CHECK_STREAM=1
scripts/run_awatch_contour_check.sh
```

Во время выполнения можно наблюдать прогресс отдельным окном:

```bash
ls -dt ~/tmp/contour-check-runs/* | head -n 1 | xargs -r -I{} tail -f {}/logs/detmir-check-json.log
```

`CONTOUR_CHECK_STREAM=1` полезен для локального запуска, если кажется, что скрипт «завис» на `== detmir-check-json ==`; фактически это длительный шаг в `detmir-check` с таймаутами по каждому endpoint.

Рекомендуемые systemd units для российского/internal контура:

```text
ops/systemd/awatch-contour-daily-check.service
ops/systemd/awatch-contour-daily-check.timer
ops/systemd/awatch-contour-weekly-check.service
ops/systemd/awatch-contour-weekly-check.timer
```

Live endpoints, hostnames, tokens and passwords must be supplied through
`/etc/detmir/detmir-check.env` (systemd units) or another private environment
file outside the public repository.

## Текущий планировщик Proxmox, проверено 2026-06-21

На Proxmox уже присутствуют следующие регулярные проверки:

| Timer | Частота | Роль |
|---|---:|---|
| `detmir-auto.timer` | каждые 30 минут | общий read-only check, AI report и safe recovery |
| `detmir-readiness-sync.timer` | каждые 10 минут | синхронизация readiness bundle |
| `detmir-portal-prewarm.timer` | каждые 30 минут | прогрев portal report cache |
| `aw-1c-clickhouse-health.timer` | каждые 5 минут | здоровье ClickHouse/1C layer |
| `aw-1c-ingest.timer` | каждые 15 минут | ingest cycle 1C/file layer |
| `aw-1c-proofcheck.timer` | каждые 6 часов | freshness proof check |
| `aw-1c-manager-brief.timer` | каждые 6 часов | manager brief |
| `aw-1c-recovery-brief.timer` | каждые 6 часов | recovery brief |
| `aw-1c-weekly-digest.timer` | понедельник 08:20 | weekly executive digest |
| `proxmox-lxc-critical-updates-check.timer` | ежедневно 03:00 | critical/important updates check |

Наблюдение: отдельный ежедневный полный gate по всей матрице AWatch-rus
отсутствует. Его роль должен закрыть `awatch-contour-daily-check.timer`.

Наблюдение: последняя проверка `detmir-portal-prewarm.service` на момент осмотра
имела `Result=exit-code` и `ExecMainStatus=28`. Это не надо маскировать:
канонический check должен показывать такой сбой как fail/warn в зависимости от
политики эксплуатации.

## Матрица требований и проверок

| Область | Что проверяется | Исполнитель | Ежедневно | Еженедельно |
|---|---|---|---:|---:|
| ActivityWatch API | `/api/0/info`, доступность API | `detmir-check` | да | да |
| Worktime API | `/reports/worktime/today` | `detmir-check` | да | да |
| 1C API | `/api/health` | `detmir-check` | да | да |
| Gateway | `/healthz` | `detmir-check` | да | да |
| Portal hardening | `/healthz`, `/readyz`, `/version`, `/metrics` | `detmir-check` | да | да |
| Windows/RDP | TCP 5985 и 22 | `detmir-check` | да | да |
| ActivityWatch buckets | AFK/window/worktime/session events | `detmir-check` | да | да |
| AWatch DLP buckets | endpoint signals/incidents/review/rules | `detmir-check` | да | да |
| AWatch DLP health | remote `dlp-health-check --json` через `detmir-dlp` | `detmir-check` | да | да |
| Grafana evidence | свежий JSON артефакт Grafana check | `detmir-check` | да | да |
| Security events backend | ClickHouse events, если включено | `detmir-check` | да | да |
| Portal contract | role/API smoke | `scripts/awatch-production-hardening-smoke.mjs` | нет | да |
| Pilot contract | demo/API smoke | `scripts/detmir-pilot-demo-smoke.mjs` | нет | да |
| Registry/readiness docs | registry readiness check | `scripts/registry_readiness_check.sh` | опционально | да |

## Fail-closed политика

Ежедневный check должен завершаться non-zero, если падает обязательная область:

- ActivityWatch API;
- Worktime API;
- Gateway/Portal health;
- RDP/Windows reachability;
- свежесть обязательных bucket streams;
- AWatch DLP health;
- Grafana evidence freshness.

Event-driven buckets не должны считаться stale только из-за отсутствия новых
инцидентов. Для них фиксируется статус `EVENT-DRIVEN`.

## Что ещё требуется довести

- Развернуть `awatch-contour-daily-check.timer` на Proxmox после установки
  обновленного `detmir-check`.
- Развернуть `awatch-contour-weekly-check.timer` для расширенного smoke/docs
  контроля.
- Заполнить `/etc/awatch-rus/contour-check.env` live значениями без записи
  секретов в репозиторий.
- Настроить retention для `.ops/contour-check-runs` или серверного output path.
- После первого успешного недельного запуска приложить summary к
  registry/readiness evidence пакету.
