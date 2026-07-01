# Эксплуатационная зрелость DetMir/AWatch-rus

Дата актуализации: 2026-07-01.

Этот контур добавляет автоматическую проверку эксплуатационной зрелости без
нагрузки на production. Public CI запускает только offline checks: fixtures,
локальный mock HTTP, статическую проверку конфигураций, ClickHouse DDL и
контракт наблюдаемости. Live checks запускаются только вручную оператором с
private access.

## Что проверяется

- Integration harness: локальный mock обслуживает ключевые endpoints
  `/healthz`, `/readyz`, `/version`, Worktime management и Security Finding
  Inbox shadow payload.
- Fault injection: клиент должен быстро классифицировать `503`, timeout и
  connection reset, не зависая сверх заданного бюджета.
- API compatibility: DetMir Portal OpenAPI обязан сохранять ключевые paths,
  schemas и runtime endpoints.
- Config/migration validation: JSON/YAML examples, systemd units/timers и
  ClickHouse init SQL проверяются на базовую пригодность и idempotency.
- Bounded load: короткий локальный load smoke с concurrency и p95 budget,
  без sizing claims.
- Observability: обязательные health/version/readiness fields, diagnostic
  headers и Prometheus metric names закреплены manifest/fixtures/source check.

## Команды

Offline PR/CI gate:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
node --check scripts/operational-maturity-check.mjs
node scripts/operational-maturity-check.mjs --json
```

Live contract, только оператором и только если контур доступен:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
AWATCH_OPS_LIVE_URL=http://127.0.0.1:8720 \
node scripts/operational-maturity-check.mjs --json --live
```

Live check не включает DLP, Loki или Velociraptor. Он только читает
health/readiness/version/metrics endpoints и принимает controlled statuses
`200` или `503`.

## Governance

Контракт расположен в `configs/operational-maturity-contract.json`.
Fixture payloads лежат в `docs/fixtures/operational-maturity/`.
CI workflow: `.github/workflows/operational-maturity.yml`.

Правило изменений: если endpoint, metric, schema, systemd unit или ClickHouse
migration меняется, сначала обновляется manifest/fixture, затем код. Удаление
полей или paths считается breaking change, если нет отдельного operator-approved
major contract change.
