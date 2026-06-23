# ClickHouse Workforce analytics for AWatch-rus / DetMir

Этот каталог содержит воспроизводимый ClickHouse-слой для привязки событий
AWatch-rus к оргструктуре, классификации приложений и доменов, а также для
быстрых агрегатов Grafana.

Слой не заменяет `clickhouse-1c/`. Это отдельный контур для workforce/web
аналитики ActivityWatch-событий.

## Состав

- `docker-compose.yml` - локальный ClickHouse scaffold.
- `clickhouse/init/00_database.sql` - база `aw_workforce`.
- `clickhouse/init/01_raw_tables.sql` - нормализованные staging tables для
  window/browser events.
- `clickhouse/init/02_dimensions_dictionaries.sql` - dimension tables и
  ClickHouse Dictionaries.
- `clickhouse/init/03_materialized_views.sql` - агрегированная таблица и
  materialized views для Grafana.
- `clickhouse/init/04_quality_views.sql` - views контроля unknown-зон.
- `sample/seed_demo.sql` - минимальные demo-данные для smoke-проверки.
- `sample/seed_sharkon2025_p3.sql` - первая реальная привязка
  `SHARKON2025/sharkon2025/user1/tsj`.
- `ops/run_smoke.sh` - локальный smoke для DDL, dictionaries и агрегатов.
- `ops/aw-workforce-ingest.service` / `.timer` - production timer для
  инкрементальной загрузки.
- `ops/aw-workforce-ingest.env.example` - переменные окружения loader-а.
- `catalog/*.tsv` - управляемые администратором справочники.
- `ops/apply_catalogs.sh` - полная загрузка справочников, reload dictionaries,
  опциональный rebuild агрегатов.
- `ops/report_unknowns.sh` - быстрый отчет top unknown users/processes/domains.

## Быстрый старт

```bash
cd clickhouse-workforce
docker compose up -d
./ops/run_smoke.sh
```

Локальный scaffold не задает `CLICKHOUSE_USER/PASSWORD` через Docker entrypoint:
это оставляет штатный dev-доступ ClickHouse без пароля и не ломает
`SOURCE(CLICKHOUSE(...))` у dictionaries. Файл
`clickhouse/users.d/99-aw-workforce-local.xml` разрешает HTTP-запросы от Docker
host, а HTTP/native порты по умолчанию привязаны только к `127.0.0.1`.

Скрипт применяет SQL в правильном порядке, загружает demo seed и проверяет:

- статус dictionaries;
- наличие hourly aggregate rows;
- daily productivity view;
- unknown quality views.

`sample/seed_demo.sql` добавляет демонстрационные строки. Для чистого повтора
локального smoke пересоздайте volume:

```bash
docker compose down -v
docker compose up -d
./ops/run_smoke.sh
```

## Production порядок

1. Реальные источники `aw_window_events` и `aw_browser_events` для
   `SHARKON2025` подтверждены:
   `docs/clickhouse/AW_WORKFORCE_SOURCES_SHARKON2025_RU.md`.
2. Настроить ingest из ActivityWatch/exporter в staging tables.
3. Загрузить `dim_workstation_user`, `dim_application_category`,
   `dim_domain_category`.
4. Проверить `system.dictionaries`.
5. Включить materialized views.
6. Перевести Grafana на `agg_workforce_productivity_hourly` и
   `v_workforce_productivity_daily`.

Исправление справочников не пересчитывает старые агрегаты автоматически.
Для исторических периодов нужен backfill по регламенту из
`docs/clickhouse/DICTIONARIES_IMPLEMENTATION_PLAN_RU.md`.

## Live ingest P2/P3

Rust loader находится в `adk-rust/crates/aw-workforce-ingest`.

Пример загрузки bounded-окна из живого AW API в локальный ClickHouse:

```bash
cargo run --manifest-path ../adk-rust/Cargo.toml -p aw-workforce-ingest -- \
  --aw-url http://10.10.10.13:5600/api/0 \
  --clickhouse-url http://127.0.0.1:8124 \
  --host SHARKON2025 \
  --hours 24 \
  --json
```

Применение первой привязки P3:

```bash
docker exec -i aw-rus-workforce-clickhouse clickhouse-client --multiquery \
  < sample/seed_sharkon2025_p3.sql
```

## Production ingest P4

В штатном режиме loader запускается без `--since/--until`: он читает
`AW_WORKFORCE_STATE_PATH`, берет `last_end - AW_WORKFORCE_OVERLAP_SECONDS`,
загружает bounded range и атомарно сохраняет новый `last_end`. Повторная
загрузка overlap-окна не удваивает данные, потому что loader перед вставкой
проверяет `source_bucket + source_event_id`.

Runtime-файлы:

```bash
cd clickhouse-workforce
sudo bash ./ops/bootstrap_runtime.sh
sudo install -m 0755 ../adk-rust/target/release/aw-workforce-ingest \
  /usr/local/bin/aw-workforce-ingest
sudo editor /etc/activitywatch/aw-workforce-ingest.env
sudo systemctl enable --now aw-workforce-ingest.timer
```

Ручная production-проверка одного цикла:

```bash
sudo systemctl start aw-workforce-ingest.service
sudo journalctl -u aw-workforce-ingest.service -n 80 --no-pager
```

## Admin workflow справочников P5

Справочники ведутся через `catalog/*.tsv`. Это полный source of truth:
`ops/apply_catalogs.sh` очищает dimension tables, загружает TSV, reload-ит
dictionaries и, если нужно, пересобирает агрегаты.

Посмотреть слепые зоны:

```bash
./ops/report_unknowns.sh
```

Добавить или изменить категорию:

```bash
editor catalog/application_categories.tsv
REBUILD_AGGREGATES=1 ./ops/apply_catalogs.sh
```

Убрать запись из отчетов без потери аудита: поставить `is_active=0` в TSV и
запустить:

```bash
REBUILD_AGGREGATES=1 ./ops/apply_catalogs.sh
```

Если менялись только future-facing справочники и старые агрегаты пересчитывать
не нужно, можно запустить без `REBUILD_AGGREGATES=1`.
