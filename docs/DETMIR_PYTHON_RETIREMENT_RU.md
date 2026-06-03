# DetMir: статус вывода Python из runtime

Дата фиксации: 2026-06-03.

## Решение

Боевой контур DetMir/AWatch-rus переводится на Rust-first runtime. Python entrypoints удаляются из серверных DLP, health, SLO, worktime, Hayabusa/offline tooling, backup/merge и deploy путей, если для них уже есть Rust-замена.

## Удалено из repo/runtime path

- AW health/SLO/worktime Python services.
- DLP policy engine, case management, compliance, integrations Python services.
- DLP aggregator, health check, admin CLI, IOC extractor, AW DB merge Python scripts.
- Hayabusa case/offline helper Python scripts.
- Legacy `aw-health-check.sh` server-side fallback.
- Active `/usr/local/bin/*.py` entrypoints and old DLP service virtualenvs on AW server.
- Python-era DLP Ansible roles now fail fast and point to `ansible/deploy_aw_server.yml`.
- Python-era Proxmox CT bootstrap playbooks now fail fast until rebuilt as Rust bootstrap.

## Оставленные исключения

- Telegram bot runtime on Proxmox remains Python by product decision.
- pfSense tooling is frozen/no-touch in this migration track.
- OCR/content-analysis keeps Python dependencies for image/OCR path; Rust handles text/dictionary/regex path.
- `clickhouse-1c/ai` and `clickhouse-1c/etl` remain a separate 1C/business-data track.
- `detmir-mcp/main.py` remains a separate MCP/runtime track.

## Проверка

Основной guard встроен в `quality-gate`:

```bash
scripts/quality-gate.sh
```

Он проверяет tracked-файлы и блокирует возврат `.py` entrypoints в Rust-retired
runtime paths (`aw-server`, `proxmox`, `scripts`, `ansible`) за исключением
согласованных зон: Telegram bot, OCR/content-analysis, 1C/AI/ETL, MCP,
pfSense/no-touch и Grafana-1C.

```bash
rg -n '\.py\b|python3' ansible aw-server scripts adk-rust \
  --glob '!adk-rust/target/**'

ansible-playbook --syntax-check -i ansible/inventory.ini ansible/deploy_aw_server.yml

cd adk-rust
CARGO_TARGET_DIR=/tmp/detmir-adk-rust-target cargo test --workspace
CARGO_TARGET_DIR=/tmp/detmir-adk-rust-target cargo clippy --workspace --all-targets -- -D warnings
```

Ожидаемые оставшиеся Python-ссылки после очистки: только перечисленные исключения, историческая документация или legacy cleanup patterns.

Production AW server scan excludes the agreed OCR/content-analysis path,
temporary external Hayabusa rules under `dlp-ioc/tmp`, and rollback backups:

```bash
find /usr/local/bin /opt/activitywatch \
  -path "/opt/activitywatch/dlp-content-analysis" -prune -o \
  -path "/opt/activitywatch/dlp-ioc/tmp" -prune -o \
  -path "*/switch-backups" -prune -o \
  \( -name "*.py" -o -name "health-check.sh" -o -path "*/.venv" \) -print
```

Ожидаемый результат для active server runtime: пустой вывод.
