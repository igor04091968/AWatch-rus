# Эксплуатационная проверка контура

Дата актуализации: 2026-06-23

Документ фиксирует минимальный профессиональный контур проверки после
существенных изменений Rust-кода, сборщиков telemetry, ClickHouse workforce
аналитики, gateway или Grafana dashboards.

## Rust / cargo gate

Выполнять из репозитория. `CARGO_TARGET_DIR` должен быть вне рабочей копии, чтобы
не загрязнять diff.

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target

cargo fmt --all --check
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo audit --deny warnings
```

Проверка политики зависимостей:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
cargo deny --manifest-path adk-rust/Cargo.toml check \
  --config deny.toml \
  --hide-inclusion-graph \
  --show-stats
```

Windows/RDP collector дополнительно проверяется под целевой ABI:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target

cargo check --target x86_64-pc-windows-gnu -p aw-windows-telemetry --locked
cargo clippy --target x86_64-pc-windows-gnu \
  -p aw-windows-telemetry \
  --all-targets \
  --locked \
  -- -D warnings
```

Repository-specific gate:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
python3 scripts/public_secret_pattern_check.py
node scripts/operational-maturity-check.mjs --json

cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo run -p quality-gate -- --root /mnt/usb_hdd2/Projects/ActivityWatch-Russian
```

Подробности эксплуатационного maturity harness: [эксплуатационная зрелость
DetMir/AWatch-rus](OPERATIONAL_MATURITY_RU.md).

## Browser smoke

Browser smoke не заменяет API/CLI проверки. Он подтверждает, что операторский
контур реально открывается в браузере и dashboards рендерят панели.

Минимальный набор страниц:

- `http://10.10.10.13:5600/` - ActivityWatch WebUI.
- `http://10.10.10.13:5610/reports/worktime/today` - дневной RDP отчет.
- `http://10.10.10.13:5610/reports/worktime/management` - управленческий RDP
  отчет.
- `http://10.10.10.2:8710/manager/brief` - 1C executive brief.
- `http://10.10.10.2:8710/manager/actions` - очередь управленческих действий.
- `http://10.10.10.2:8710/manager/recovery` - recovery brief.
- `http://10.10.10.2:8710/manager/digest/weekly` - weekly digest.
- `https://dm.iri1968.dpdns.org/` - gateway index через Basic Auth.
- `https://dm.iri1968.dpdns.org/d/detmir-rdp-user-activity/detmir3a-rabota-pol-zovatelej-v-rdp?orgId=1&from=now-7d&to=now&timezone=browser&var-host=SHARKON2025&refresh=5m`
  - RDP user activity dashboard.
- `https://dm.iri1968.dpdns.org/d/detmir-aw-main/detmir3a-activitywatch-overview?orgId=1&from=now-24h&to=now&timezone=browser&refresh=5m`
  - main ActivityWatch dashboard.

Правила:

- Basic Auth читать с `pve-detmir:/etc/detmir/proxmox-web-gateway.credentials`.
  Пароль нельзя печатать в логах, документации, commit messages или final report.
- Скриншоты для диагностики хранить в `/tmp/aw-browser-smoke-*`; не коммитить.
- Ошибки `404` по Grafana endpoint `/api/dashboards/uid/*/public-dashboards`
  не считаются отказом панели: это metadata public dashboard, не datasource.
  Пустые panels, 5xx, ошибки datasource или отсутствие данных в body/screenshot
  считаются регрессией.

## Production smoke

После deploy или изменения telemetry/workforce выполнить:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
NO_PROXY=localhost,127.0.0.1,10.10.10.13,10.10.10.2,192.168.100.19,10.10.10.0/24 \
no_proxy=localhost,127.0.0.1,10.10.10.13,10.10.10.2,192.168.100.19,10.10.10.0/24 \
AW_SMOKE_WINDOWS_HOST=192.168.100.19 \
AW_SMOKE_SOURCE_HOSTNAME=SHARKON2025 \
./check-aw-full.sh
```

Для DetMir production при проверке после rename RDP-сервера явно фиксируйте
stable logical host id:

```bash
AW_MONITORED_WINDOWS_HOSTNAME=SHARKON2025 ./check-aw-data.sh
AW_SMOKE_WINDOWS_HOST=192.168.100.19 \
AW_SMOKE_SOURCE_HOSTNAME=SHARKON2025 \
./scripts/aw-contour-smoke-local.sh --skip-winrm
```

`SHARKON2025` в этих командах - исторический ActivityWatch logical id, не
физическое имя Windows-сервера.

Для workforce ClickHouse дополнительно проверить quality views:

```sql
SELECT
  (SELECT count() FROM aw_workforce.v_workforce_unknown_subjects) AS unknown_subjects,
  (SELECT count() FROM aw_workforce.v_workforce_unknown_processes) AS unknown_processes,
  (SELECT count() FROM aw_workforce.v_workforce_unknown_domains) AS unknown_domains,
  (SELECT countIf(user_login = 'unknown'
    OR (process_name = 'unknown' AND lengthUTF8(window_title) = 0))
   FROM aw_workforce.aw_window_events) AS no_user_window_rows;
```

Ожидаемое состояние после cleanup/normalization: все четыре значения равны `0`.

## Retention / cleanup validation

Политика хранения описана в [Retention and Cleanup Policy](RETENTION_POLICY_RU.md).
Перед изменением сроков хранения или включением нового cleanup scope сначала
выполнить dry-run и сохранить вывод в change evidence.

Проверить активные timers:

```bash
systemctl list-timers \
  aw-prune-local-state.timer \
  aw-db-maintenance.timer \
  aw-db-vacuum.timer \
  detmir-readiness.timer
```

Dry-run локальной очистки:

```bash
sudo AW_DATA_DIR=/var/lib/activitywatch \
  AW_WORKTIME_REPORT_DISK_CACHE_DIR=/var/lib/activitywatch/worktime-report-cache \
  AW_WORKTIME_REPORT_DISK_STALE_TTL_SECONDS=86400 \
  /usr/local/bin/aw-prune-local-state-rust --json
```

Проверить, что в planned items нет:

- production database outside `/var/lib/activitywatch/backups/db`;
- config/env files;
- dashboards;
- Hayabusa forensic archive;
- DLP evidence/cases/compliance reports;
- release evidence;
- Windows queues.

Apply разрешен только после dry-run review:

```bash
sudo /usr/local/bin/aw-prune-local-state-rust --apply --json
```

Проверить журналы cleanup и DB maintenance:

```bash
journalctl \
  -u aw-prune-local-state.service \
  -u aw-db-maintenance.service \
  -u aw-db-vacuum.service \
  -u detmir-readiness.service \
  -n 160 --no-pager
```

Оценить disk usage до и после:

```bash
du -sh \
  /var/lib/activitywatch \
  /var/log/activitywatch \
  /opt/hayabusa \
  /opt/activitywatch/clickhouse-1c \
  /opt/activitywatch/clickhouse-workforce 2>/dev/null || true

docker system df 2>/dev/null || true
```

Для ClickHouse 1C/workforce проверить размер таблиц без удаления данных:

```bash
docker exec aw-rus-1c-clickhouse clickhouse-client --query \
  "SELECT database, table, formatReadableSize(sum(bytes_on_disk)) AS size FROM system.parts WHERE active GROUP BY database, table ORDER BY sum(bytes_on_disk) DESC" \
  2>/dev/null || true

docker exec aw-rus-workforce-clickhouse clickhouse-client --query \
  "SELECT database, table, formatReadableSize(sum(bytes_on_disk)) AS size FROM system.parts WHERE active GROUP BY database, table ORDER BY sum(bytes_on_disk) DESC" \
  2>/dev/null || true
```

Проверить, что cleanup не повлиял на running services:

```bash
systemctl status activitywatch-server aw-worktime-api --no-pager
curl -fsS http://127.0.0.1:5600/api/0/info >/dev/null
curl -fsS http://127.0.0.1:5610/healthz >/dev/null
```

Windows EVTX retention проверяется отдельно, потому что выполняется на RDP host:

```powershell
powershell.exe -ExecutionPolicy Bypass `
  -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1 `
  -RetentionDays 14
```

Запрещено вручную удалять Windows collector queues, incident artifacts, DLP
evidence, Hayabusa archives, Grafana data или ClickHouse tables без отдельного
operator approval и backup/restore plan.

## Проверка отсутствия ClickHouse-пароля в argv

Цель проверки - убедиться, что ClickHouse/1C runtime wrappers не передают
`CLICKHOUSE_PASSWORD` через аргументы процессов. Пароль должен поступать из
`/opt/activitywatch/clickhouse-1c/.env` в окружение или временный client config,
а не через `--password`.

Статическая проверка wrappers:

```bash
rg -n -- '--password[= ]+"?\$[{]?CLICKHOUSE_PASSWORD' \
  /opt/activitywatch/clickhouse-1c/ops
```

Ожидаемый результат: команда не выводит совпадений.

Runtime smoke во время ingest/brief refresh:

```bash
set -a
. /opt/activitywatch/clickhouse-1c/.env
set +a

ps -eo args= | grep -E 'clickhouse-client|generate_.*brief|refresh_company' |
while IFS= read -r line; do
  if printf '%s' "${line}" | grep -F -- "${CLICKHOUSE_PASSWORD}" >/dev/null; then
    echo "FAIL: ClickHouse password is visible in process argv" >&2
    exit 1
  fi
done
```

Ожидаемый результат: команда завершается с кодом `0` и не печатает секрет.
Если проверка падает, остановить rollout и вернуть предыдущий release artifact.
