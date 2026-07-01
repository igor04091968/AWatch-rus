# Эксплуатационная проверка контура

Дата актуализации: 2026-07-01

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

Dependency hygiene для production/high-load Rust contour:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust

cargo metadata --locked --format-version 1 >/tmp/aw-rus-cargo-metadata.json
cargo tree --duplicates --locked > /tmp/aw-rus-cargo-tree-duplicates.txt
cargo update --locked
cargo audit --deny warnings

cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
cargo deny --manifest-path adk-rust/Cargo.toml check \
  --config deny.toml \
  --hide-inclusion-graph \
  --show-stats
```

`cargo update --locked` используется как drift probe: он не должен менять
`Cargo.lock`, но может показать доступные совместимые обновления и завершиться
ошибкой из-за `--locked`. Такой результат фиксировать как сигнал для отдельного
dependency-refresh PR, а не как повод молча обновлять lockfile в функциональной
ветке.

Периодически, особенно перед dependency cleanup или production binary refresh,
проверять неиспользуемые зависимости:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
cargo machete
```

Удалять найденные зависимости только после ручной проверки через `rg` и
целевого `cargo test -p <crate> --locked`: `cargo machete` может ошибаться на
feature-gated, generated или metadata-driven usage.

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

cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo run -p quality-gate -- --root /mnt/usb_hdd2/Projects/ActivityWatch-Russian
```

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
