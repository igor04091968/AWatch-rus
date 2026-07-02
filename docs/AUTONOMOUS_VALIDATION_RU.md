# Автономная проверка AWatch-rus

Документ описывает единый validation entry point для длительной
эксплуатационной проверки AWatch-rus/DetMir без ручного надзора.

Цель framework - не менять production runtime, а регулярно доказывать состояние
репозитория, release artifacts, operational maturity, runtime health и evidence.

## Запуск

Из корня репозитория:

```bash
scripts/run_full_validation.sh --profile full
```

Профили:

- `quick` - repository, JSON/YAML/Markdown, retention/recovery docs, secret and
  contract checks.
- `standard` - `quick` плюс operational maturity, deployment readiness, pilot
  validation, release artifact checks, binary parity self-test.
- `full` - `standard` плюс Rust, Cargo и dependency hygiene gates.
- `runtime` - `standard` с акцентом на runtime diagnostics и health endpoints.

Для unattended запуска можно задать каталог reports:

```bash
AW_VALIDATION_OUTPUT_DIR=/var/lib/activitywatch/validation \
AW_VALIDATION_RETENTION_DAYS=45 \
scripts/run_full_validation.sh --profile standard
```

## Production evidence inputs

Framework не собирает production binary parity evidence самостоятельно. Это
намеренное ограничение: оператор сначала формирует approved evidence JSON, затем
gate проверяет его в репозитории.

```bash
PRODUCTION_BINARY_PARITY_EVIDENCE=/var/lib/activitywatch/evidence/production-binary-parity.json \
scripts/run_full_validation.sh --profile standard
```

Release evidence проверяется только когда явно указан каталог evidence package:

```bash
AW_RELEASE_EVIDENCE_DIR=/var/lib/activitywatch/release-evidence/rc-1 \
scripts/run_full_validation.sh --profile standard
```

## Runtime diagnostics

Каждый запуск собирает machine-readable диагностику:

- CPU/load average;
- RAM/swap;
- disk usage;
- размеры известных runtime paths;
- queue directories;
- systemd unit state, restart count, exit status;
- ActivityWatch, Worktime, Grafana, Prometheus, ClickHouse endpoints.
- TLS certificate expiry for hosts listed in `AW_VALIDATION_TLS_HOSTS`.

Endpoint defaults безопасны для локального запуска:

- `AW_VALIDATION_ACTIVITYWATCH_URL`, default
  `http://127.0.0.1:5600/api/0/info`;
- `AW_VALIDATION_WORKTIME_URL`, default `http://127.0.0.1:5610/healthz`;
- `AW_VALIDATION_GRAFANA_URL`;
- `AW_VALIDATION_PROMETHEUS_URL`;
- `AW_VALIDATION_CLICKHOUSE_URL`.
- `AW_VALIDATION_TLS_HOSTS`, comma-separated `host:port` list.

ActivityWatch и Worktime по умолчанию проверяются на localhost. Grafana,
Prometheus, ClickHouse и TLS hosts проверяются только если заданы. Если заданный
endpoint недоступен, создается machine-readable alert.

## Reports

Каждый запуск пишет reports в:

```text
output/validation/history/<timestamp>/
output/validation/latest/
```

Файлы:

- `validation-report.json`;
- `validation-report.md`;
- `production-health.json`;
- `deployment-health.json`;
- `runtime-health.json`;
- `release-evidence.json`;
- `operational-evidence.json`;
- `recovery-evidence.json`;
- `build-evidence.json`;
- `operational-dashboard.json`;
- `operational-dashboard.md`.

Все JSON reports содержат timestamp, Git SHA, host identifier, profile,
validation outcome и evidence summary.

## Regression detection

Framework сравнивает текущий запуск с предыдущим report из history и фиксирует:

- новые failures;
- восстановленные failures;
- ухудшение длительности checks;
- изменение `Cargo.lock`;
- configuration drift;
- unexpected release artifact changes;
- disk/database/log path growth above threshold;
- новые alerts.

Первый запуск создает baseline. Дальнейшие запуски пишут regression summary в
`validation-report.json` и `validation-report.md`.

## Alert rules

Alerts создаются при:

- failed validation;
- недоступном настроенном health endpoint;
- превышении disk threshold;
- превышении memory threshold;
- failed systemd unit;
- restart count больше нуля;
- queue backlog выше threshold;
- отсутствии production binary parity evidence;
- отсутствии release evidence directory;
- expired или missing backup evidence path, если он задан;
- secret pattern check failure;
- configuration drift;
- unexpected artifact changes;
- certificate expiry within `AW_VALIDATION_CERT_EXPIRY_WARN_DAYS`.

Thresholds:

```bash
AW_VALIDATION_DISK_USED_WARN_PCT=85
AW_VALIDATION_DISK_USED_CRIT_PCT=95
AW_VALIDATION_MEMORY_USED_WARN_PCT=90
AW_VALIDATION_MEMORY_USED_CRIT_PCT=97
AW_VALIDATION_CERT_EXPIRY_WARN_DAYS=14
AW_VALIDATION_PATH_GROWTH_WARN_BYTES=1073741824
AW_VALIDATION_PATH_GROWTH_WARN_RATIO=1.25
AW_VALIDATION_QUEUE_FILES_WARN=1000
AW_VALIDATION_BACKUP_EVIDENCE_PATH=/var/lib/activitywatch/evidence/latest-backup.json
AW_VALIDATION_BACKUP_EVIDENCE_MAX_AGE_DAYS=7
```

## Scheduling

Пример systemd timer должен выполняться вне hot path и не должен включать DLP,
Loki или always-on Velociraptor:

```ini
[Service]
Type=oneshot
WorkingDirectory=/mnt/usb_hdd2/Projects/ActivityWatch-Russian
Environment=AW_VALIDATION_OUTPUT_DIR=/var/lib/activitywatch/validation
Environment=AW_VALIDATION_RETENTION_DAYS=45
ExecStart=/mnt/usb_hdd2/Projects/ActivityWatch-Russian/scripts/run_full_validation.sh --profile standard
```

Для weekly deep validation использовать `--profile full`, потому что Rust и
dependency gates могут быть дорогими.

## Failure investigation

1. Открыть `output/validation/latest/validation-report.md`.
2. Проверить `Alerts`.
3. Для failed command смотреть `validation-report.json`: `stdout`, `stderr`,
   `exit_code`, `timed_out`.
4. Если alert относится к runtime endpoint, проверить соответствующий service
   через runbook [эксплуатационной проверки](OPERATIONS_VALIDATION_RUNBOOK_RU.md).
5. Если alert относится к binary parity, обновить production evidence JSON и
   повторить запуск.
6. Если alert относится к release evidence, проверить controlled release package
   через registry runbook.

## History retention

История хранится в `history/<timestamp>/`. Старые запуски удаляются по
`AW_VALIDATION_RETENTION_DAYS` или `--retention-days`.

Минимальное значение retention: 1 день. Для production рекомендуется 30-45
дней, чтобы видеть недельные тренды по runtime failures, restart frequency,
disk growth и validation score.
