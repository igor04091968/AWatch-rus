# Ежедневное обслуживание AWatch-rus

Дата актуализации: 2026-07-02

Документ описывает ежедневный операторский цикл обслуживания AWatch-rus /
DetMir и безопасное использование Pollinations AI как вспомогательного
аналитика evidence-артефактов. Pollinations не является production runtime
dependency, не принимает решения автоматически и не имеет прямого доступа к
customer contour.

## Цели

- Подтвердить, что сервисы отвечают и не находятся в скрытой деградации.
- Проверить свежесть данных, состояние очередей, timers, retention и readiness
  evidence.
- Сформировать короткий дневной maintenance summary.
- Использовать Pollinations только для сжатия и первичного анализа локальных
  JSON/Markdown evidence без секретов.

## Guardrails

- Не включать heavy DLP, Loki или always-on Velociraptor в рамках ежедневного
  обслуживания.
- DLP runtime для DetMir production считать нормальным в light/disabled profile,
  если оператор отдельно не принял решение о включении.
- Не удалять ClickHouse, Grafana, DLP evidence, Hayabusa archives, Windows
  queues, release evidence и diagnostic bundles вручную без отдельного change.
- Pollinations failure не является отказом AWatch-rus. Если AI недоступен,
  ежедневное обслуживание продолжается по обычному runbook.
- Не отправлять в Pollinations secrets, private env files, bearer tokens,
  пароли, raw customer evidence, user identifiers, скриншоты с ПДн или полные
  журналы production.

## Ежедневный цикл

### 1. Быстрый статус сервисов

Проверить локальный health/readiness и failed units:

```bash
curl -fsS http://127.0.0.1:5600/api/0/info >/dev/null
curl -fsS http://127.0.0.1:5610/healthz >/dev/null
systemctl --failed --no-pager
systemctl list-timers \
  aw-prune-local-state.timer \
  aw-db-maintenance.timer \
  aw-db-vacuum.timer \
  detmir-readiness.timer
```

Критерий: ActivityWatch и Worktime API отвечают, нет неожиданных failed units,
ежедневные timers присутствуют в расписании.

### 2. Readiness bundle

Проверить, что ежедневный bundle сформирован и проверяется:

```bash
cd /var/lib/activitywatch/health/readiness-bundle
sha256sum -c sha256sums.txt
openssl dgst -sha256 -verify public-key.pem \
  -signature sha256sums.txt.sig sha256sums.txt
jq -r '.status // .overall_status // empty' detmir-readiness-status.json
```

Критерий: checksum/signature проходят, статус не `FAIL`, дата bundle относится
к текущему maintenance window.

### 3. Свежесть telemetry и reports

Проверить штатные smoke/validation команды из репозитория:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
node scripts/operational-maturity-check.mjs --json
node scripts/pilot-validation-smoke.mjs
node scripts/deployment-readiness-smoke.mjs
```

Если включён автономный validation orchestrator, использовать его как единый
источник daily evidence:

```bash
./scripts/run_full_validation.sh --profile standard
```

Критерий: нет новых hard-fail проверок. Warning допустим только при известном
и задокументированном operational constraint.

### 4. Очереди, backlog и storage growth

Проверить отсутствие неконтролируемого роста:

```bash
du -sh /var/lib/activitywatch /var/log/activitywatch 2>/dev/null || true
docker system df 2>/dev/null || true
docker exec aw-rus-1c-clickhouse clickhouse-client --query \
  "SELECT database, table, formatReadableSize(sum(bytes_on_disk)) AS size FROM system.parts WHERE active GROUP BY database, table ORDER BY sum(bytes_on_disk) DESC" \
  2>/dev/null || true
```

Для Windows queues не выполнять ручную очистку. Если backlog растёт, сначала
проверить доступность AW server, scheduled tasks и upload logs.

### 5. Retention safety

Проверить dry-run cleanup, если есть подозрение на рост диска или после
изменения retention-конфигурации:

```bash
sudo AW_DATA_DIR=/var/lib/activitywatch \
  AW_WORKTIME_REPORT_DISK_CACHE_DIR=/var/lib/activitywatch/worktime-report-cache \
  AW_WORKTIME_REPORT_DISK_STALE_TTL_SECONDS=86400 \
  /usr/local/bin/aw-prune-local-state-rust --json
```

Apply не является ежедневной обязательной операцией. Apply выполнять только
после просмотра planned items и подтверждения, что cleanup не затрагивает
configuration, dashboards, forensic/security evidence, release evidence или
Windows queues.

### 6. Короткая запись результата

Сохранить дневной summary в operator journal или internal change log:

```text
date:
operator:
health:
readiness_bundle:
telemetry_freshness:
storage:
retention:
new_alerts:
known_constraints:
actions_taken:
```

## Pollinations AI assistant

### Роль

Pollinations используется только как локально запускаемый помощник для:

- сжатия daily evidence в операторский summary;
- выделения новых failures/warnings из JSON/Markdown reports;
- подготовки списка вопросов для ручной проверки;
- сравнения текущего maintenance summary с предыдущим summary.

Pollinations не должен:

- запускать production commands самостоятельно;
- получать secrets/private env/raw logs;
- менять конфигурацию;
- принимать решение о cleanup apply, DLP enablement, restart или rollback.

### Проверка доступности CLI

```bash
/home/igor/.local/bin/polli-chat --help
/home/igor/.local/bin/polli-agent --help
/home/igor/.local/bin/polli-chat --max-tokens 16 'ping'
```

Для `polli-agent` использовать `--collection`, не `--db`:

```bash
/home/igor/.local/bin/polli-agent index \
  --collection /tmp/awatch-maintenance-index \
  /path/to/sanitized-evidence.md \
  /path/to/sanitized-validation-report.json

/home/igor/.local/bin/polli-agent ask \
  --collection /tmp/awatch-maintenance-index \
  --context-only \
  'Какие новые failures или warnings есть в сегодняшнем evidence?'
```

### Sanitization перед AI-анализом

Перед передачей в Pollinations оставить только технические поля:

- status/outcome;
- timestamps;
- Git SHA/version;
- service names;
- check names;
- counters;
- sizes;
- non-sensitive error class;
- documented operational constraints.

Удалить или заменить placeholders:

- usernames and user identifiers;
- tokens/passwords/API keys;
- Basic Auth credentials;
- private IP details, если summary планируется публиковать;
- screenshots and raw event payloads;
- full journal lines with command arguments.

### Prompt для daily summary

```text
Ты анализируешь sanitized AWatch-rus daily maintenance evidence.
Не придумывай факты. Не предлагай включать DLP/Loki/always-on Velociraptor.
Раздели вывод на:
1. OK
2. New warnings
3. New failures
4. Evidence gaps
5. Required operator actions
Если evidence недостаточно, напиши "не доказано".
```

### Fail-closed правило

Если Pollinations summary расходится с raw evidence, считать источником истины
raw evidence и runbook. AI-вывод можно использовать только как черновик для
операторского анализа.

## Журнал выполненного обслуживания

### 2026-07-03: production cleanup, collector guard и dependency hardening

Выполнено:

- очищены старые ActivityWatch SQLite backup files на AW server с сохранением
  последних двух rollback-точек;
- `/` на AW server снижен с `85%` до `40%`, свободно `19G`;
- заменен Windows production binary
  `C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe`;
- заменен gateway/ClickHouse production binary
  `/usr/local/bin/aw-1c-ingest-rust`;
- закрыт RustSec риск `quick-xml` через удаление `calamine` из
  `aw-1c-ingest` и переход на прямое чтение XLSX через `zip` +
  `quick-xml 0.41.0`;
- добавлен regression test на разбор registry XLSX листов `Лист2` и
  `ОСНОВНОЙ`.

Production checksums:

| Binary | SHA256 |
|---|---|
| `aw-windows-telemetry.exe` | `A8F517AB636C8537413201B84EA6540CB198045F388FECE6C8D6810537FBDDF1` |
| `aw-1c-ingest-rust` | `e0c4e665ea8946ba9c243127a65fd69554cacdc4e3191ec798b8ca19acf89623` |

Rollback files:

- `C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe.bak.20260702T202626Z`;
- `/usr/local/bin/aw-1c-ingest-rust.bak.20260702T211637Z`;
- `/usr/local/bin/aw-1c-ingest-rust.bak.20260702T212258Z`.

Post-change production evidence:

- `check-aw-full.sh`: `FRESH=8`, `STALE=0`, `DEAD=0`;
- `aw-rus-healthd`: `ok=True`;
- AW server `systemctl --failed`: `0 loaded units listed`;
- `pve-detmir systemctl --failed`: `0 loaded units listed`;
- `aw-1c-ingest.service`: `Result=success`, `ExecMainStatus=0`,
  `files_loaded=8`, `rows_loaded=561`;
- ClickHouse container `aw-rus-1c-clickhouse`: `healthy`;
- gateway `/healthz`: `ok`.

Validation executed before documenting:

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo audit --no-fetch
cargo deny --manifest-path adk-rust/Cargo.toml check --config deny.toml --hide-inclusion-graph --show-stats
cargo metadata --locked --format-version 1
cargo tree --duplicates --locked
cargo machete --with-metadata
python3 scripts/public_secret_pattern_check.py
node scripts/operational-maturity-check.mjs --json
git diff --check
```

Cargo/RustSec network note: if direct `https://index.crates.io` is unavailable
from the laptop, use the local HTTP CONNECT proxy:

```bash
export HTTP_PROXY=http://127.0.0.1:10808
export HTTPS_PROXY=http://127.0.0.1:10808
export http_proxy=http://127.0.0.1:10808
export https_proxy=http://127.0.0.1:10808
unset ALL_PROXY all_proxy
```

## Эскалация

Эскалировать как production issue, если:

- `/healthz`, `/readyz` или Worktime reports не отвечают;
- readiness bundle не сформирован или не проходит checksum/signature;
- есть неожиданные failed systemd units;
- storage growth продолжается после штатного retention;
- Windows queues растут и upload не восстанавливается;
- DLP/security evidence удалено, повреждено или изменилось без change;
- Pollinations обнаружил warning, который подтверждается raw evidence.

## Связанные документы

- [Operations Runbook](OPERATIONS_RUNBOOK_RU.md)
- [Эксплуатационная проверка контура](OPERATIONS_VALIDATION_RUNBOOK_RU.md)
- [Retention and Cleanup Policy](RETENTION_POLICY_RU.md)
- [Production readiness](PRODUCTION_READINESS_RU.md)
- [Autonomous validation](AUTONOMOUS_VALIDATION_RU.md)
