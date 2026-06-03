# Runbook: перевод DetMir на Rust / ADK-Rust

Дата фиксации: `2026-06-01`

Цель: постепенно заменить хрупкие Python/shell operational scripts на
самодостаточные Rust-бинарники, не ломая текущий production-контур DetMir.

Этот runbook является рабочим планом миграции. Если фактический runtime
расходится с этим документом, сначала фиксируется baseline, затем обновляется
runbook.

Текущий resume snapshot проекта хранится в
`.ai/runtime/detmir-current-session.md`. Перед продолжением миграции после
перерыва или context reset сначала читать его, затем `RUNBOOK.md` и
`<OPERATOR_CODEX_HOME>/skills/detmir-rust-migration/references/current-state.md`.

## 1. Целевое состояние

В результате миграции DetMir должен иметь:

- единый Rust workspace в `adk-rust/`;
- один standalone-бинарник или crate на каждую операционную функцию;
- стабильные machine-readable JSON-контракты вместо парсинга human text;
- одинаковые exit codes для старых и новых команд;
- systemd units/timers, не зависящие от ноутбука, venv, pip и локальных путей;
- безопасный auto-heal с dry-run, lock, cooldown, allowlist и rollback;
- ADK-compatible envelopes там, где вывод передается агентам/боту/LLM.

Rust не должен использоваться как самоцель. Если компонент надежнее оставить в
Ansible, PowerShell или Playwright, он остается там до появления практической
причины для переноса.

## 2. Текущая отправная точка

Уже создано:

- `adk-rust/Cargo.toml` - workspace;
- `adk-rust/crates/detmir-status` - read-only status module with text, JSON and ADK JSON output;
- `/usr/local/bin/detmir-status` на Proxmox, проверен против
  `/var/lib/detmir-ai/latest-state.json`;
- `/usr/local/bin/detmir-adk-status` на Proxmox, проверен против
  `/var/lib/detmir-ai/latest-state.json` как compatibility binary.

Ключевые текущие legacy-компоненты:

| Компонент | Текущая реализация | Риск переноса |
|---|---|---|
| `detmir-status` | Rust binary + thin compatibility wrapper | выполнено |
| `detmir-check` | Rust binary + thin compatibility wrapper | выполнено |
| `detmir-dlp` | Rust SSH wrapper + thin compatibility wrapper | выполнено |
| `detmir-auto` | Rust production via systemd drop-in + legacy script retained | switched |
| `detmir-heal-safe` | Rust binary deployed + legacy script retained | switched for Rust auto |
| `tsj_guardian_watchdog.sh` | Rust service via systemd drop-in + legacy shell retained | switched |
| Telegram `/status`/decision backend | Rust helper + permanent Python bot runtime | backend only |
| `aw-rus-healthd` | Rust production unit, Python entrypoint removed from repo | done |
| `dlp-health-check` | Rust production binary, Python entrypoint removed from repo | done |
| DLP aggregator | Rust production via systemd drop-in + legacy Python retained | switched |
| AW DLP Influx exporter | Rust production via systemd drop-in + legacy Python retained | switched |
| AW worktime Influx exporter | Rust production via systemd drop-in + legacy Python retained | switched |
| AW worktime prewarm | Rust production via systemd drop-in + legacy shell retained | switched |
| AW worktime API | Rust production via systemd drop-in + legacy Python retained | switched |
| DLP syslog forwarder | Rust production via systemd drop-in + legacy Python retained | switched |
| DLP webhook sender | Rust production via systemd drop-in + legacy Python retained | switched |
| DLP CEF exporter | Rust production via systemd drop-in + legacy Python retained | switched |
| `tsj_guardian_bot.py` | Permanent Python Telegram runtime | не переносить |
| AW remaining worktime modules | mostly Rust production; inspect leftovers before next item | средний |
| deploy/install scripts | shell/Ansible/PowerShell | высокий, переносить последними |

## 3. Архитектура workspace

Целевая структура:

```text
adk-rust/
  Cargo.toml
  Cargo.lock
  RUNBOOK.md
  crates/
    detmir-core/
    detmir-aw-client/
    detmir-systemd/
    detmir-state/
    detmir-status/
    detmir-check/
    detmir-dlp/
    detmir-auto/
    detmir-heal-safe/
    tsj-guardian-status/
    tsj-guardian-watchdog/
```

Назначение shared crates:

| Crate | Назначение |
|---|---|
| `detmir-core` | ошибки, CLI output, timeouts, retry, exit codes, config loading |
| `detmir-aw-client` | ActivityWatch HTTP API, buckets, events, timestamps |
| `detmir-systemd` | безопасный wrapper вокруг `systemctl`, allowlist, dry-run |
| `detmir-state` | чтение/запись `/var/lib/detmir-ai`, atomic writes, retention |
| `detmir-report` | Markdown/JSON reports, ADK envelopes, report bundle assembly |

Бинарные модули должны быть маленькими и собираться из shared crates. Логику не
дублировать между `check`, `status`, `auto` и ботом.

## 4. Общие контракты для каждого Rust-модуля

Каждый новый модуль обязан иметь:

- `--json` для машинного вывода;
- `--pretty` или обычный text output для человека, если команда операторская;
- `--config <path>` либо documented env vars;
- `--dry-run` для любых действий, которые меняют состояние;
- `--timeout` там, где есть сеть/SSH/HTTP;
- `--no-color` если вывод может попадать в systemd/Telegram/report;
- стабильные exit codes:
  - `0` - OK;
  - `1` - usage/config/runtime error;
  - `2` - проверка выполнена, но состояние WARN/FAIL;
  - `3` - action запрещен safety policy;
- structured logs через `tracing` для daemon/action-команд;
- unit tests на чистую логику;
- fixture tests на реальные JSON samples;
- README с примером локального и серверного запуска.

## 5. Safety policy

Нельзя сразу заменять управляющие скрипты без shadow-mode.

Обязательные правила:

- сначала read-only parity, потом mutation;
- старый и новый модуль должны некоторое время работать параллельно;
- новый модуль не получает право писать state, рестартить сервисы или удалять
  файлы до прохождения acceptance gates;
- все state writes только через atomic temp file + rename;
- heal-команды только по allowlist units;
- no implicit sudo: если нужен `sudo`, он должен быть явно виден в deploy/unit;
- lock file обязателен для `auto` и `heal`;
- cooldown обязателен для restart/start actions;
- каждый risky action пишет audit entry;
- rollback должен быть одной командой systemd/service symlink switch.

## 6. Фазы миграции

### Phase 0. Baseline и фиксация контрактов

Цель: перед переносом зафиксировать, что именно считается корректным поведением.

Действия:

1. Снять текущие outputs legacy-команд:
   - `detmir-status`;
   - `detmir-check --json`;
   - `detmir-dlp`;
   - `detmir-auto` на зеленом контуре;
   - `detmir-heal-safe` в dry-safe сценарии или на mock host.
2. Сохранить sanitized fixtures в `adk-rust/fixtures/`.
3. Зафиксировать JSON schemas:
   - status summary;
   - DetMir check result;
   - DLP health result;
   - auto run state;
   - heal action log.
4. Зафиксировать exit code matrix старых команд.
5. Зафиксировать runtime paths:
   - `/var/lib/detmir-ai`;
   - `/usr/local/bin`;
   - relevant systemd units/timers.

Гейт готовности:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo check --workspace
```

Результат фазы: миграция не начинается вслепую; есть baseline для сравнения.

### Phase 1. Rust foundation

Цель: создать shared crates, чтобы не плодить разные реализации HTTP, времени,
ошибок, JSON и systemd.

Модули:

- `detmir-core`;
- `detmir-state`;
- `detmir-aw-client`;
- `detmir-systemd`.

Минимальная функциональность:

- RFC3339/UTC timestamp parsing;
- HTTP JSON client with timeout/retry;
- TCP check;
- atomic JSON write;
- retention cleanup;
- exit-code helper;
- common `StatusLevel`: `OK`, `WARN`, `FAIL`;
- bucket mode enum: `fresh`, `inactive_ok`, `event_driven`;
- systemd read-only checks;
- systemd action allowlist, но без включения mutation by default.

Гейт:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Phase 2. Read-only operator modules

Цель: заменить самые безопасные команды, не меняющие состояние.

Порядок:

1. `detmir-status`
   - объединить с совместимым `detmir-adk-status`;
   - читать `/var/lib/detmir-ai/latest-state.json`;
   - выдавать text, JSON, ADK JSON;
   - заменить shell wrapper после parity.
2. `detmir-check`
   - перенести Python checks в Rust;
   - сохранить семантику `FRESH`, `INACTIVE`, `EVENT-DRIVEN`, `STALE`, `DEAD`;
   - сохранить `summary.bucket_ok/stale/dead/service_failures/service_warnings`;
   - не менять thresholds без отдельного решения.
3. `detmir-dlp`
   - сначала переписать SSH wrapper;
   - затем отдельно переносить `dlp-health-check.py` на AW server.

Shadow-mode для `detmir-check`:

```bash
detmir-check --json > /tmp/detmir-check.old.json
adk-rust/target/release/detmir-check --json > /tmp/detmir-check.new.json
jq -S '.summary' /tmp/detmir-check.old.json > /tmp/old.summary.json
jq -S '.summary' /tmp/detmir-check.new.json > /tmp/new.summary.json
diff -u /tmp/old.summary.json /tmp/new.summary.json
```

Гейт:

- old/new agree on `ok`;
- old/new agree on summary counters;
- new module returns same exit code class;
- no network side effects;
- server run succeeds from systemd-like environment.

### Phase 3. State/report orchestration

Цель: заменить `detmir-auto` без немедленной замены heal.

Первый Rust `detmir-auto` должен:

- брать lock;
- создавать run directory;
- запускать `detmir-check` и `detmir-dlp`;
- писать `summary-before.json`;
- писать `state-*.json`;
- обновлять symlinks:
  - `latest-run`;
  - `latest-state.json`;
  - `latest-report.md`;
- выполнять retention cleanup;
- уметь `--no-heal`;
- по умолчанию на первом этапе не выполнять heal, а только писать
  `would_heal=true`.

Pollinations/LLM report:

- не должен быть hard dependency для статуса;
- failure LLM report не должен ломать state update;
- raw JSON summary всегда должен сохраняться даже при ошибке report generation.

Гейт:

- Rust `detmir-auto --no-heal` дает такой же `latest-state.json` по смыслу;
- symlink updates atomic enough for readers;
- timer можно прогнать вручную без изменения heal behavior;
- `detmir-status` видит новое state без изменений.

Текущее состояние:

- `detmir-auto-rust` установлен на Proxmox как shadow binary
  `/usr/local/bin/detmir-auto-rust`;
- `detmir-auto-rust` умеет вызывать Rust heal через `--enable-heal` или
  `DETMIR_AUTO_HEAL=1`, но shadow unit явно запускается с `--no-heal`;
- production `detmir-auto.service` переключен на `/usr/local/bin/detmir-auto-rust`
  через drop-in `/etc/systemd/system/detmir-auto.service.d/20-rust-switch.conf`;
- legacy `/usr/local/bin/detmir-auto` сохранен для rollback;
- отдельный `detmir-auto-rust-shadow.timer` отключен после успешного
  production timer observation; unit-файлы и shadow state сохранены;
- shadow пишет только в `/var/lib/detmir-ai/shadow/detmir-auto-rust`;
- `detmir-auto-rust-shadow.service` использует `SuccessExitStatus=2`, чтобы
  найденный shadow FAIL фиксировался в JSON, но не загрязнял
  `systemctl --failed`;
- перед стартом shadow service проверяет, что production `detmir-auto.service`
  не активен;
- последняя systemd shadow-проверка после retry в AW client:
  `severity=OK`, `check_rc=0`, `dlp_rc=0`, buckets `8/0/0`, DLP `22/0/0`.
- production Rust start проверен: `ExecStart=/usr/local/bin/detmir-auto-rust
  --command-timeout-seconds 180`, latest-state `OK`, `systemctl --failed`
  пустой.
- scheduled production timer cycle проверен после switch: process status `0`,
  latest-state `OK`, report generated, `systemctl --failed` пустой.

### Phase 4. Safe heal

Цель: перенести `detmir-heal-safe`, но только после read-only parity.

Обязательные guardrails:

- `--dry-run` default на первых deploy;
- allowlist units:
  - `activitywatch-server.service`;
  - `aw-worktime-api.service`;
  - `aw-worktime-ui-bridge.timer`;
  - `activitywatch-dlp-aggregator.timer` только если unit существует;
- запрет wildcard restart;
- `systemctl reset-failed` только для allowlist;
- cooldown между restart попытками;
- audit log в run directory;
- hard timeout на SSH/systemctl;
- never touch Windows/RDP recovery from this module.

Гейт:

- dry-run показывает exact actions;
- на зеленом контуре не рестартит ничего;
- на mock/failing unit рестартит только allowlisted unit;
- после heal всегда запускается повторный check;
- exit code не маскирует FAIL.

Текущее состояние:

- создан crate `detmir-heal-safe`;
- binary развернут на Proxmox как `/usr/local/bin/detmir-heal-safe-rust`;
- production `/usr/local/bin/detmir-heal-safe` пока не заменен;
- production Rust auto использует `/usr/local/bin/detmir-heal-safe-rust` через
  `DETMIR_HEAL_BIN`;
- default mode: dry-run, mutation только через `--apply`;
- optional timer start выключен по умолчанию, включается только
  `--start-optional`;
- green dry-run проверен: restart/start не планируются;
- green apply проверен: выполнены только `reset-failed` и DLP health snapshot,
  контур остался `OK`, `systemctl --failed` пустой.
- red/mock heal test проверен на Proxmox в isolated state-dir:
  `summary-before=FAIL`, heal вызван, check/dlp повторены, final state `OK`,
  process rc `0`.

### Phase 5. Watchdog и lightweight services

Цель: заменить простые shell watchdogs и wrappers.

Порядок:

1. `tsj-guardian-watchdog.sh` -> `tsj-guardian-watchdog`;
2. lightweight service wrappers;
3. bounded cleanup/retention jobs.

Особое внимание:

- bot heartbeat file parsing;
- duplicate `gost` instance dedupe;
- service restart only on stale/missing heartbeat;
- no killing unrelated processes.

Гейт:

- dry-run mode показывает, какой PID был бы убит;
- process matching покрыт тестами;
- systemd timer/service rollback сохранен.

Текущее состояние:

- создан crate `tsj-guardian-watchdog`;
- binary развернут на Proxmox как
  `/usr/local/bin/tsj-guardian-watchdog-rust`;
- production `tsj-guardian-watchdog.service` переключен на Rust через drop-in
  `/etc/systemd/system/tsj-guardian-watchdog.service.d/20-rust-switch.conf`;
- legacy `/opt/infra-admin/tsj-bot/tsj_guardian_watchdog.sh` сохранен;
- dry-run выявил bug legacy-подхода: `pgrep -f` не матчился как literal из-за
  `+` в `gost` pattern;
- Rust версия использует literal process scan через `ps`;
- one-shot apply удалил лишний duplicate `gost` PID и оставил systemd MainPID;
- service mode работает как:

```bash
/usr/local/bin/tsj-guardian-watchdog-rust --apply --loop-forever --interval-seconds 60
```

Rollback:

```bash
sudo rm -f /etc/systemd/system/tsj-guardian-watchdog.service.d/20-rust-switch.conf
sudo systemctl daemon-reload
sudo systemctl restart tsj-guardian-watchdog.service
```

### Phase 6. AW server health and DLP modules

Цель: перенести серверные Python health checks/exporters.

Порядок:

1. `aw-server/aw-rus-healthd.py` - выполнено, production через
   `aw-rus-healthd.service.d/20-rust-switch.conf`;
2. `scripts/dlp-health-check.py` - выполнено, `/usr/local/bin/dlp-health-check`
   заменён Rust-бинарником;
3. `scripts/aggregate_dlp_events.py` - выполнено, production через
   `activitywatch-dlp-aggregator.service.d/20-rust-switch.conf`;
4. DLP integrations:
   - syslog forwarder - выполнено, production через
     `aw-dlp-syslog-forwarder.service.d/20-rust-switch.conf`;
   - webhook sender - выполнено, production через
     `aw-dlp-webhook-sender.service.d/20-rust-switch.conf`;
   - CEF exporter - выполнено, production через
     `aw-dlp-cef-exporter.service.d/20-rust-switch.conf`;
5. `aw-server/aw-dlp-influx-exporter.py` - выполнено, production через
   `aw-dlp-influx-exporter.service.d/20-rust-switch.conf`;
6. `aw-server/aw-worktime-influx-exporter.py` - выполнено, production через
   `aw-worktime-influx-exporter.service.d/20-rust-switch.conf`;
7. `aw-server/aw-worktime-prewarm.sh` - выполнено, production через
   `aw-worktime-prewarm.service.d/20-rust-switch.conf`;
8. `aw-server/aw-worktime-ui-bridge.py` - выполнено, production через
   `aw-worktime-ui-bridge.service.d/20-rust-switch.conf`;
9. `aw-server/aw-worktime-autoheal.sh` - выполнено, production через
   `aw-worktime-autoheal.service.d/20-rust-switch.conf`;
10. remaining worktime modules.

Правила:

- exporters должны сохранять Prometheus/Influx output format;
- API endpoints не менять без compatibility layer;
- для каждого exporter сначала golden output fixture;
- service unit меняется только после side-by-side run.

Гейт:

- old/new metrics names match;
- dashboard datasource health не ломается;
- unit tests старого поведения перенесены или сохранены;
- `systemctl --failed` clean после deploy.

### Phase 7. Telegram bot backend helpers

Цель: не переносить Telegram bot runtime на Rust. Python остается постоянным
production runtime для Telegram polling/sending, proxy, retries и side effects.
На Rust выносятся только backend helpers, status aggregation, decision/gating и
безопасные read-only/action contracts, которые бот вызывает как внешние команды.

Порядок:

1. `[done]` Бот продолжает жить на Python.
2. `[started]` Команды `/status`, `/detmir`, health summaries начинают читать Rust JSON.
   Первым вынесен `tsj-guardian-status`: read-only helper для строки
   `detmir_auto` из `/var/lib/detmir-ai/latest-state.json`; вторым шагом тот же
   helper начал рендерить `aw_rus_slo` из `AW_RUS_SLO_SUMMARY_CMD`.
3. Recovery/actions вызывают Rust binaries с dry-run/audit.
4. `[decision]` Bot runtime на Rust не переносится; новые улучшения Telegram
   делаются в Python runtime или через Rust backend helpers.

Почему так:

- Telegram runtime, proxy, retries и async edge cases уже стабильно покрыты
  Python-кодом и тестами;
- перенос runtime даст мало пользы и высокий риск регрессий;
- максимальная польза от Rust сначала в backend-командах и safety contracts.

Гейт:

- `/status` показывает те же строки или лучше;
- smoke message проходит;
- watchdog активен;
- restart bot не теряет config/secrets;
- секреты не попадают в logs/tests.

### Phase 8. Deploy/install scripts

Цель: переносить последними, только после стабилизации runtime binaries.

Что может остаться не на Rust:

- Ansible playbooks;
- Windows PowerShell collector deployment;
- Playwright browser smoke;
- packaging scripts, если они надежны и редко исполняются.

Что имеет смысл перенести:

- validation CLI;
- install-kit consistency checker;
- local report generators;
- deterministic packaging helpers.

Гейт:

- install-kit output byte/content expected;
- rollback installer path documented;
- production deploy не зависит от laptop-only paths.

## 7. Deployment workflow

Сборка:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
```

Установка одного бинарника на Proxmox:

```bash
install -o root -g root -m 0755 \
  target/release/<binary> \
  /usr/local/bin/<binary>
```

Через Ansible:

```bash
ansible proxmox -i ansible/inventory.ini -m copy -a \
  'src=/mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust/target/release/<binary> dest=/usr/local/bin/<binary> owner=root group=root mode=0755 backup=yes'
```

Проверка на сервере:

```bash
<binary> --version || true
<binary> --help
<binary> <safe-read-only-command> --json
systemctl --failed
```

## 8. Rollback workflow

Для каждого заменяемого компонента должен быть rollback:

1. Старый script остается на сервере как `<name>.legacy` или в package backup.
2. systemd unit меняется через drop-in или controlled template.
3. Перед switch:
   - сохранить `systemctl cat <unit>`;
   - сохранить checksum старого binary/script;
   - сохранить latest known good command.
4. Rollback command документируется в PR/run note.

Текущий rollback для `detmir-auto` после Rust switch:

```bash
sudo rm -f /etc/systemd/system/detmir-auto.service.d/20-rust-switch.conf
sudo systemctl daemon-reload
sudo systemctl restart detmir-auto.service
```

Перед switch сохранены:

```bash
/var/lib/detmir-ai/switch-backups/detmir-auto.service.before-rust-20260531-192809.txt
/var/lib/detmir-ai/switch-backups/detmir-auto.sha256.before-rust-20260531-192809.txt
```

Пример:

```bash
cp -a /usr/local/bin/detmir-check /usr/local/bin/detmir-check.legacy
install -o root -g root -m 0755 target/release/detmir-check /usr/local/bin/detmir-check
detmir-check --json
```

Если новый binary провален:

```bash
mv /usr/local/bin/detmir-check.legacy /usr/local/bin/detmir-check
systemctl restart detmir-auto.timer || true
```

## 9. Acceptance checklist

Перед заменой legacy-команды:

- [ ] old/new CLI documented;
- [ ] old/new JSON compared on real fixture;
- [ ] exit codes match by class;
- [ ] unit tests pass;
- [ ] integration smoke pass on target host;
- [ ] no secrets in logs;
- [ ] systemd environment tested;
- [ ] rollback command known;
- [ ] docs updated;
- [ ] operator command example added.

Перед включением mutation/recovery:

- [ ] `--dry-run` verified;
- [ ] allowlist verified;
- [ ] lock verified;
- [ ] cooldown verified;
- [ ] audit log verified;
- [ ] green contour causes no action;
- [ ] failing contour causes only intended action;
- [ ] post-action check verified;
- [ ] rollback tested.

## 10. Quality gates

Локально:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

На Proxmox:

```bash
detmir-status --json
detmir-check --json
detmir-dlp
detmir-auto --no-heal
systemctl --failed
```

На AW server:

```bash
aw-rus-healthd --json
dlp-health-check --json
/usr/local/bin/aw-slo-monitor-rust --state-dir /var/lib/activitywatch/slo-rust-shadow --json
systemctl --failed
```

Для Telegram:

```bash
systemctl is-active tsj-guardian-bot tsj-guardian-watchdog gost-tg
```

## 11. Приоритет ближайших работ

Немедленный порядок:

1. `[done]` Создать `detmir-core`, `detmir-state`, `detmir-aw-client`.
2. `[done]` Объединить `detmir-status` и `detmir-adk-status` в один Rust binary.
3. `[done]` Перенести `detmir-check`.
4. `[done]` Сделать shadow compare `detmir-check` old/new.
5. `[done]` Перенести `detmir-dlp` wrapper.
6. `[done]` Начать `detmir-auto --no-heal`.
7. `[done]` Накопить scheduled shadow/prod parity для `detmir-auto-rust`, затем выполнить controlled switch.
8. `[done]` Перенести `detmir-heal-safe` с dry-run/apply guardrails и подключить к Rust auto.
9. `[done]` Наблюдать timer цикл Rust production и отключить redundant shadow timer.
10. `[done]` Перенести `tsj-guardian-watchdog.sh` на Rust service loop.
11. `[done]` Перенести AW-RUS SLO monitor/summary path на Rust:
    `aw-slo-monitor-rust` пишет `aw-slo-samples.jsonl`,
    `aw-slo-summary.json` и `aw-slo-summary.txt`; production включен через
    `/etc/systemd/system/aw-slo-monitor.service.d/20-rust-switch.conf`.
12. `[done]` Выполнить controlled correction старых ложных SLO bad-сэмплов:
    backup создан, 133 записи с единственной причиной
    `worktime_today_html body too small ... < 5000` исправлены, 59
    не-ложных/неоднозначных bad-сэмплов оставлены в истории.
13. `[done]` Вынести Telegram `/status` aggregation layer в Rust:
    `tsj-guardian-status --status-text` собирает полный текст статуса, Python
    runtime Telegram оставлен и использует старую сборку только как fallback.
14. `[done]` Вынести incident/escalation decision path в Rust:
    `tsj-guardian-status` принимает JSON через stdin и решает suggestions,
    transient quorum/defer и timeout escalation/fallback. Python runtime
    выполняет только side effects и сохраняет fallback на старую логику.
15. `[done]` Вынести operator action routing/gating в Rust:
    `tsj-guardian-status --operator-action-decision` нормализует action aliases,
    назначает handler/risk, блокирует неизвестные action и защищает
    update install/rollback confirm без pending-запроса. Python runtime
    продолжает выполнять команды и сохраняет fallback.
16. `[done]` Вынести DLP policy/mode decision path в Rust:
    `tsj-guardian-status --dlp-policy-decision` определяет monitor/enforce/mixed,
    считает block/total по endpoint groups, строит toggle/target policy plan и
    changed_rules. Python runtime продолжает выполнять API PUT и Windows policy
    sync, fallback на старую Python-логику сохранен.
17. `[done]` Вынести confirmation state machines в Rust:
    `tsj-guardian-status --confirmation-decision` решает TTL/expire, cancel,
    first_confirm и apply code/stage validation для pfSense, OpenVPN,
    Proxmox selection и Proxmox restore. Python runtime продолжает создавать
    pending-запросы и выполнять side effects после `allowed=true`, fallback на
    старую Python-валидацию сохранен.
18. `[done]` Вынести autoheal/action plan extraction в Rust:
    `tsj-guardian-status --autoheal-plan-decision` классифицирует AW-Rus
    failures в Windows collector heal, server-side DLP heal, worktime rebuild,
    SLO no-direct-target и задержку перед повторной проверкой. Python runtime
    продолжает выполнять recovery-команды и повторный probe, fallback на старую
    Python-маршрутизацию сохранен. Production deploy проверен на Proxmox:
    sample decisions OK, `tsj-guardian-bot`, `tsj-guardian-watchdog` и
    `gost-tg` active, `detmir-status` OK / `ok_for_operator=true`, свежий
    journal после правильной выкладки helper без повторных unexpected-argument
    warnings.
19. `[done]` Укрепить deploy artifacts после миграции:
    `deploy_aw_server.yml` переведен на единый `aw_rust_release_dir` для Rust
    binary `stat`/`copy`, чтобы `CARGO_TARGET_DIR` работал одинаково для
    Proxmox/Telegram и AW-server deploy. Добавлен
    `scripts/check_detmir_rust_release_artifacts.sh`; проверено
    `CARGO_TARGET_DIR=<OPERATOR_HOME>/.cache/detmir-adk-rust-target cargo build
    --release --workspace`, artifact check OK, `deploy_aw_server.yml
    --syntax-check` OK.
20. `[done]` Прогнать production AW-server deploy contract и закрыть найденные
    эксплуатационные дефекты:
    - Influx tokens для deploy берутся из окружения; при восстановлении с
      сервера значения не печатать.
    - Legacy DB merge выключен по умолчанию через
      `aw_legacy_db_merge_enabled: false`, потому что backup SQLite на
      12G root FS уперся в `No space left on device`. Повторять merge только
      после отдельного disk plan.
    - Browser smoke по умолчанию принудительно использует `chromium-cli`;
      `node-playwright` на текущем Node ломается `rimraf: callback function
      required`.
    - `aw-rus-healthd-rust` теперь проверяет AW interactive buckets по
      `metadata.end`, а event buckets выбирают свежайшее событие из окна
      `limit=20` с учетом `timestamp + duration`; это устраняет ложные
      stale/warn на длинных ActivityWatch событиях и guard heartbeat ordering.
    - После reset runtime budget `AWatchRusCollectorGuard` и однократного
      перезапуска AW watcher tasks production verification зеленый:
      `aw-rus-healthd-rust` `ok=14 warn=0 fail=0`, `detmir-status` OK,
      `systemctl --failed` на AW и Proxmox пусто, `deploy_aw_server.yml
      --syntax-check` OK.
21. `[done]` Выполнить безопасную уборку AW app data и защитить timers от
    лавинного восстановления:
    - удалены только старые browser-smoke run-директории, manual profile,
      неиспользуемый `/opt/activitywatch/releases/v0.13.2` и неиспользуемый
      `/opt/activitywatch/aw-rus-ops/venv`;
    - текущий `aw-server-rust-v0.13.2`, SQLite DB/WAL/SHM и rollback-critical
      backups не удалялись;
    - root FS улучшен примерно с `96%` до `85%`;
    - AW timers переведены на staggered `OnCalendar`, чтобы после ручного
      restart/daemon-reload не запускать все missed persistent jobs одной
      пачкой;
    - legacy `aw_to_influx_exporter.service` удален из systemd, потому что
      production уже использует Rust DLP/worktime Influx exporters;
    - `aw-rus-healthd-rust` получил timeout для wrapper-команд, чтобы
      `aw-health-check`/`dlp-health-check` не могли подвесить healthd.
22. `[done]` Перенести DLP content analyzer text path в Rust:
    - добавлен `dlp-content-analyzer` crate;
    - CLI совместим с `aw-dlp-content-analyzer --text --dictionary-pack
      --regex-pack`;
    - реализованы dictionary/regex pack matching и checksum validators для
      ИНН, СНИЛС, паспорта РФ;
    - позиции совпадений считаются в Python-compatible character offsets, не
      byte offsets;
    - image/OCR path оставлен через legacy Python fallback, чтобы не тащить
      OCR/Tesseract в Rust на этом шаге;
    - `/usr/local/bin/aw-dlp-content-analyzer` теперь wrapper, который
      предпочитает `/usr/local/bin/aw-dlp-content-analyzer-rust`, но сохраняет
      Python fallback;
    - parity на AW server проверен на `152-fz-pdn`, `contacts`, `secrets`,
      `financial`: match counts, values и offsets совпали;
    - artifact check расширен `dlp-content-analyzer`.
23. `[done]` Перенести DLP admin CLI в Rust:
    - добавлен `dlp-admin-cli` crate;
    - сохранены команды `policies list/active`, `incidents list`,
      `cases list/create`, `health check`;
    - HTTP client использует no-proxy для локальных AW/DLP сервисов;
    - `/usr/local/bin/dlp-admin-cli` установлен как Rust production CLI,
      legacy `/usr/local/bin/dlp-admin-cli.py` больше не является repo/runtime
      path; рабочий CLI: `/usr/local/bin/dlp-admin-cli`;
    - parity на AW server совпал с Python для `health`, `policies active`,
      `incidents list --since-hours 24 --limit 5`, `cases list --limit 5`;
    - production verification зеленый: AW failed units 0,
      `aw-rus-healthd-rust` 14/0/0, `dlp-health-check` 22/0/0,
      `detmir-status` OK, `detmir-check` OK, Proxmox failed units 0;
    - artifact check расширен `dlp-admin-cli`.
24. `[done]` Перенести DLP policy engine service в Rust:
    - добавлен `dlp-policy-engine` crate;
    - сохранены существующая SQLite schema/DB и API: policies
      list/create/get/update/delete, active bundle/version,
      submit/approve/draft/activate, rollback, agent heartbeat/desired,
      audit;
    - checksum policy совместим с Python:
      `json.dumps(..., ensure_ascii=False, sort_keys=True,
      separators=(",", ":"))` + SHA-256;
    - env contract сохранен:
      `AW_DLP_POLICY_ENGINE_BIND_HOST`, `AW_DLP_POLICY_ENGINE_PORT`,
      `AW_DLP_POLICY_ENGINE_DB_PATH`;
    - shadow parity на AW server совпал с Python для list/active/version и
      agent heartbeat/desired;
    - Rust исправляет documented `/api/0/dlp/policies/audit?limit=N`: legacy
      FastAPI возвращал 422 из-за route-order конфликта с `{policy_id}`;
    - production `/usr/local/bin/aw-dlp-policy-engine-rust` подключен через
      `/etc/systemd/system/aw-dlp-policy-engine.service.d/20-rust-switch.conf`;
    - rollback: удалить этот drop-in, `systemctl daemon-reload`, restart
      `aw-dlp-policy-engine.service`;
    - production verification зеленый: active policy `default-policy` v6,
      audit endpoint OK, AW failed units 0, `aw-rus-healthd-rust` 14/0/0,
      `dlp-health-check` 22/0/0, `detmir-status` OK, `detmir-check` OK,
      Proxmox failed units 0;
    - artifact check расширен `dlp-policy-engine`.
25. `[done]` Перенести DLP case management service в Rust:
    - добавлен `dlp-case-management` crate;
    - сохранены существующая SQLite schema/DB и API: `/health`, cases
      list/create/get/update, comments list/create, Hayabusa forensics link;
    - сохранены self-test rejection и evidence SHA-256 contract:
      `json.dumps(..., ensure_ascii=False, sort_keys=True,
      separators=(",", ":"))` + SHA-256;
    - env contract сохранен:
      `AW_DLP_CASE_BIND_HOST`, `AW_DLP_CASE_PORT`, `AW_DLP_CASE_DB_PATH`;
    - local HTTP smoke на temp DB проверил create/list/patch/comment/hayabusa
      link/self-test reject;
    - shadow parity на AW server совпал с Python для health, case list/filter,
      case detail и comments на копии production DB;
    - production `/usr/local/bin/aw-dlp-case-management-rust` подключен через
      `/etc/systemd/system/aw-dlp-case-management.service.d/20-rust-switch.conf`;
    - rollback: удалить этот drop-in, `systemctl daemon-reload`, restart
      `aw-dlp-case-management.service`;
    - production verification зеленый: case list OK/latest id 35,
      AW failed units 0, `aw-rus-healthd-rust` 14/0/0, `dlp-health-check`
      22/0/0, `detmir-status` OK, `detmir-check` OK, Proxmox failed units 0;
    - artifact check расширен `dlp-case-management`.
26. `[done]` Перенести DLP compliance report scheduler/generator в Rust:
    - добавлен `dlp-compliance` crate;
    - сохранены CLI `--month`, `--profile`, `--profiles`, `--stdout-json`;
    - сохранены env fallback для AW API: `AW_DLP_AW_API_BASE`,
      `AW_SERVER_URL`, default `http://127.0.0.1:5600`;
    - сохранены output/template envs и artifact names:
      `<profile>-<YYYY-MM>.html/json`;
    - shadow parity на AW server в temp output dirs совпал с Python для
      `152-fz,pci-dss`: profile/period/aw_api_base/stats и наличие artifacts;
    - production `/usr/local/bin/aw-dlp-compliance-rust` подключен через
      `/etc/systemd/system/aw-dlp-report-scheduler.service.d/20-rust-switch.conf`;
    - rollback: удалить этот drop-in и `systemctl daemon-reload`;
    - production oneshot run успешен: `152-fz-2026-06` и `pci-dss-2026-06`
      html/json созданы, timer active, AW failed units 0,
      `aw-rus-healthd-rust` 14/0/0, `dlp-health-check` 22/0/0,
      `detmir-status` OK, `detmir-check` OK, Proxmox failed units 0;
    - artifact check расширен `dlp-compliance`.
27. `[done]` Перенести Hayabusa case alert/link helpers в Rust:
    - добавлен `hayabusa-tools` crate с бинарниками
      `aw-hayabusa-case-alert-rust` и `aw-hayabusa-link-case-rust`;
    - сохранены CLI/env контракты legacy helpers:
      `--case-id`, `--intake-json`, `--case-api-base`, `--mode`,
      `--link-source`, `AW_HAYABUSA_*`;
    - `case-alert` сохраняет scoring/severity, `top_rules`, comment text,
      case create/update/link path и Telegram alert text;
    - Telegram request errors в Rust не печатают bot token/URL;
    - `/usr/local/bin/aw-hayabusa-case-alert` и
      `/usr/local/bin/aw-hayabusa-link-case` теперь wrapper-команды, которые
      предпочитают Rust binaries и fallback на Python из
      `/opt/activitywatch/aw-rus-ops/hayabusa`;
    - shadow compare на реальном `/opt/hayabusa/state/latest-intake.json` с
      `AW_HAYABUSA_AUTO_CASE_ENABLED=false` и
      `AW_HAYABUSA_TELEGRAM_ENABLED=false` дал `summary_equal=true`;
    - production verification зеленый: AW failed units 0,
      `aw-rus-healthd-rust` 14/0/0, `dlp-health-check` 22/0/0,
      `detmir-status` OK, Proxmox failed units 0;
    - artifact check расширен `aw-hayabusa-case-alert-rust` и
      `aw-hayabusa-link-case-rust`.
28. `[done]` Перенести remaining Hayabusa offline helpers в Rust:
    - `hayabusa-tools` расширен бинарниками
      `aw-hayabusa-from-windows-rust` и `aw-hayabusa-autoprocess-rust`;
    - `from-windows` сохраняет server-side workflow:
      Ansible WinRM export, latest zip discovery, fetch в drop-dir,
      `aw-hayabusa accept`, `process-inbox`, optional link-case и печать
      `LATEST_INTAKE`;
    - `autoprocess` сохраняет drop-dir scanner, lock file,
      `.caseid`/`.meta.json` sidecars, host/mode/link_source inference,
      case-alert capture, sidecar/archive move в report dir;
    - `/usr/local/bin/aw-hayabusa-from-windows` и
      `/usr/local/bin/aw-hayabusa-autoprocess` теперь wrapper-команды,
      которые предпочитают Rust и fallback на Python из
      `/opt/activitywatch/aw-rus-ops/hayabusa`;
    - `aw-hayabusa-drop.service` теперь запускает
      `/usr/local/bin/aw-hayabusa-autoprocess` напрямую, без `python3`, чтобы
      wrapper мог выбрать Rust;
    - no-mutation smoke на AW server: `--help` для всех Hayabusa helpers,
      empty drop-dir для `autoprocess`, `systemctl cat` ExecStart OK;
    - production verification зеленый: AW failed units 0,
      `aw-rus-healthd-rust` 14/0/0, `dlp-health-check` 22/0/0,
      `detmir-status` OK, Proxmox failed units 0;
    - artifact check расширен `aw-hayabusa-from-windows-rust` и
      `aw-hayabusa-autoprocess-rust`.
29. `[done]` Перенести AW ops helper `aw-health-check` в Rust:
    - добавлен crate `aw-health-check`;
    - сохранены проверки legacy shell: systemd services/timer,
      ActivityWatch `/api/0/info`, Worktime `/health`, DLP transport
      freshness через `dlp-health-check --json`, и drift checks для
      `startOfDay`, `always_active_pattern`, `landingpage`;
    - env contract читается из процесса и `/etc/activitywatch/aw-server.env`;
    - production `/usr/local/bin/aw-health-check` теперь Rust-required wrapper:
      вызывает `/usr/local/bin/aw-health-check-rust` и падает с ошибкой, если
      Rust binary отсутствует;
    - shadow parity на AW server совпал с shell по exit code и ключевым
      health-строкам;
    - production verification зеленый: `aw-health-check` OK,
      `aw-rus-healthd-rust` 14/0/0, `dlp-health-check` 22/0/0,
      `detmir-status` OK, `ok_for_operator=true`, failed units 0;
    - artifact check расширен `aw-health-check`.
30. `[done]` Перенести AW ops helper `check-aw-data` в Rust:
    - добавлен crate `check-aw-data`;
    - корневой `check-aw-data.sh` стал Rust-first wrapper с fallback на
      `scripts/legacy/check-aw-data.sh`;
    - на AW server production `/usr/local/bin/check-aw-data` теперь wrapper,
      который предпочитает `/usr/local/bin/check-aw-data-rust`, по умолчанию
      использует `http://127.0.0.1:5600` и fallback на
      `/opt/activitywatch/aw-rus-ops/check-aw-data.sh`;
    - сохранены операторские статусы `FRESH`, `STALE`, `DEAD`, `EMPTY`,
      `EVENT-DRIVEN`, `INACTIVE` и CORS probe;
    - быстрый режим по умолчанию использует `metadata.end` из
      `/api/0/buckets`; глубокое чтение event ids включается флагом
      `--with-event-ids`;
    - production verification: `/usr/local/bin/check-aw-data --no-color`
      завершился за 2s, buckets fresh/event-driven/inactive as expected,
      `CORS: OK (HTTP 200)`;
    - final gates зеленые: AW failed units 0, `aw-rus-healthd-rust` 14/0/0,
      `dlp-health-check` 22/0/0, `detmir-auto --no-heal` OK,
      `detmir-status` OK и `ok_for_operator=true`;
    - artifact check расширен `check-aw-data`.
31. `[done]` Перенести AW maintenance helper `aw-prune-local-state` в Rust:
    - добавлен crate `aw-prune-local-state`;
    - `/usr/local/bin/aw-prune-local-state.sh` теперь Rust-first wrapper,
      который без аргументов запускает Rust binary в legacy-compatible
      `--apply` режиме и fallback на shell из
      `/opt/activitywatch/aw-rus-ops/aw-prune-local-state.sh`;
    - Rust binary по умолчанию работает как dry-run, поддерживает `--json`,
      `--apply`, retention/keep параметры и строгий allowlist путей;
    - safety policy: не удалять SQLite DB вне `backups/db`, не удалять
      `switch-backups`, `before-rust`, `rollback` и корневые state каталоги;
    - cleanup расширен на старые `browser-smoke` run-директории как project
      app data; rollback-critical backups и AW SQLite DB не трогались;
    - production recovery во время проверки: rootfs AW server был 100%
      заполнен, `aw-server-rust` попал в `poisoned lock`; через Rust helper
      удалены старые browser-smoke runs на 347 MB суммарно, затем
      `activitywatch-server.service` восстановлен без удаления DB;
    - final gates зеленые: AW failed units 0, `aw-rus-healthd-rust` 14/0/0,
      `dlp-health-check` 22/0/0, `check-aw-data` OK, `detmir-auto --no-heal`
      OK, `detmir-status` OK и `ok_for_operator=true`;
    - Telegram bot/watchdog временно остановлены по операторской команде и не
      перезапускались в рамках этого шага;
    - artifact check расширен `aw-prune-local-state`.
32. `[done]` Устранить нехватку места на AW server через Proxmox resize:
    - CT `203` (`aw-server`, `<AW_SERVER_HOST>`) rootfs расширен через Proxmox
      `pct resize 203 rootfs +20G`;
    - перед resize сохранен config backup:
      `/var/lib/detmir-ai/switch-backups/ct203-aw-server.before-rootfs-resize-20260602T054826Z.conf`;
    - rootfs изменился с `12G` на `32G`, свободное место стало около `20G`;
    - Proxmox storage `local-btrfs` после resize имеет около `106G`
      доступного места;
    - AW `/api/0/info` и `/api/0/buckets` проверены с AW server и Proxmox;
    - final gates зеленые: AW health 14/0/0, DLP health 22/0/0,
      `detmir-check` service_warnings=0, `detmir-auto --no-heal` OK,
      `detmir-status` OK и `ok_for_operator=true`, failed units 0;
    - Telegram bot/watchdog оставлены `inactive` по операторской команде.
33. `[done]` Остановить неконтролируемый рост `aw-session-events`:
    - read-only SQLite audit показал, что `/var/lib/activitywatch/aw-server-rust/sqlite.db`
      занимает около `6.8G`, `freelist_count=0`; VACUUM сам по себе не
      освободит место, потому что размер занят live events;
    - основной источник роста: `aw-session-events_HOST-EXAMPLE` - около
      `6.9M` строк и `~5GB` payload, с пиками `1.2M-2.4M` process-level
      событий в сутки за 2026-05-29..2026-06-01;
    - production live config на RDP был `pollSeconds=5` и
      `sessionEvents.processEventsEnabled=true`;
    - отключена постоянная process-level публикация:
      `aw_windows_process_events_enabled=false` в production/example/default
      Ansible vars и `sessionEvents.processEventsEnabled=false` в live
      `C:\ProgramData\AWatch-rus\deployment-config.json`;
    - live config backup сохранен на RDP:
      `C:\ProgramData\AWatch-rus\switch-backups\deployment-config.before-disable-process-events-20260602T061836Z.json`;
    - старый `worktime-session-collector.ps1` PID `8476` остановлен, collector
      поднят заново штатными `ActivityWatch Launch [...]` tasks/guard;
    - delta-gate: `metadata.end` bucket `aw-session-events_HOST-EXAMPLE`
      остался `2026-06-02T06:27:11.197Z` через 75 секунд, постоянный поток
      остановлен;
    - базовый сбор не сломан: `detmir-check --json` OK,
      `detmir-status` severity OK, AW failed units 0, Proxmox failed units 0,
      AW API и Worktime API отвечают;
    - SQLite row deletion/compact не выполнялись на этом этапе: сначала
      стабилизирован источник роста, retention/trim старых process events -
      отдельный destructive этап с backup.
34. `[done]` Controlled trim старых process-level `aw-session-events`:
    - перед mutation сохранен полный rollback backup SQLite DB/WAL/SHM:
      `/var/lib/activitywatch/backups/db/aw-sqlite-before-session-events-trim-20260602T064127Z`;
    - на время операции остановлены AW-related timers/services и
      `activitywatch-server.service`, чтобы не было writer'ов к SQLite;
    - scoped delete удалил только события bucket
      `aw-session-events_HOST-EXAMPLE` с `eventType=process_start` или
      `eventType=process_stop`;
    - удалено `6,906,190` шумных process-level событий;
    - сохранены logon events: после trim в `aw-session-events_HOST-EXAMPLE`
      осталось `174` события, recent samples имеют `eventType=logon`;
    - `PRAGMA integrity_check` до и после `VACUUM`: `ok`;
    - DB уменьшилась с `6.8G` до `350M`, rootfs AW server вернулся к
      `38%` использования и около `19G` free;
    - после restart AW был `poisoned lock` из-за timer запросов во время
      старта; выполнен controlled restart при остановленных timers, затем
      `/api/0/info` и `/events?limit=1` вернулись OK;
    - timers включены обратно, failed units на AW и Proxmox: `0`;
    - final gates зеленые: `aw-health-check` OK, `dlp-health-check`
      `22/0/0`, `detmir-check` bucket_ok=8 stale=0 dead=0,
      `detmir-auto --no-heal` OK, `detmir-status` severity OK и
      `ok_for_operator=true`;
    - Telegram bot/watchdog оставлены `inactive` по операторской команде.
35. `[done]` Добавить Rust guard от повторного роста AW SQLite:
    - добавлен crate `aw-db-health`;
    - helper read-only открывает `/var/lib/activitywatch/aw-server-rust/sqlite.db`
      через SQLite read-only flags и не меняет DB;
    - проверяет:
      - размер `sqlite.db` (`warn=2GiB`, `fail=5GiB` по умолчанию);
      - размер `sqlite.db-wal` (`warn=256MiB`, `fail=1GiB`);
      - число строк `aw-session-events_<host>` (`warn=10000`, `fail=100000`);
      - recent process-level events за окно 600 секунд
        (`warn=1`, `fail=100`);
      - latest `eventType/source` для быстрой диагностики;
      - опционально `--windows-config` и
        `sessionEvents.processEventsEnabled`;
    - поддерживает text output и `--json`;
    - `aw-health-check-rust` теперь вызывает `/usr/local/bin/aw-db-health --json`
      и включает DB growth guard в общий AW health path;
    - `scripts/check_detmir_rust_release_artifacts.sh` требует
      `aw-db-health`;
    - `ansible/deploy_aw_server.yml` устанавливает
      `/usr/local/bin/aw-db-health` и обновленный
      `/usr/local/bin/aw-health-check-rust`; установка health helpers
      продублирована до Influx token assert, чтобы DB guard обновлялся даже
      если playbook позже останавливается на пустом внешнем Influx token;
    - production `/usr/local/bin/aw-db-health --json`:
      `ok=true`, `db=352.3MiB`, `wal=4.0MiB`, session rows `174`,
      recent process events `0`, latest `eventType=logon`;
    - production `/usr/local/bin/aw-health-check`: `AW DB growth guard passed`;
    - local gates: `cargo fmt --all -- --check`, `cargo test -p aw-db-health`
      `3 passed`, `cargo test -p aw-health-check` `2 passed`,
      release build OK, artifact check OK;
    - final gates зеленые: `dlp-health-check` `22/0/0`, `detmir-check` OK,
      `detmir-auto --no-heal` OK, `detmir-status` severity OK,
      failed units на AW/Proxmox `0`;
    - Telegram bot/watchdog оставлены `inactive`.
35.1. `[done]` Добавить автономную guarded чистку AW SQLite:
    - добавлен crate `aw-db-maintenance`;
    - helper dry-run по умолчанию, mutation только с `--apply`;
    - allowlist удаления: только bucket `aw-session-events_<host>` и только
      события `eventType=process_start` или `eventType=process_stop`;
    - logon/session events не удаляются;
    - retention по умолчанию `7` дней;
    - перед удалением создается SQLite backup в
      `/var/lib/activitywatch/backups/db/aw-sqlite-before-db-maintenance-*.db`;
      если строк к удалению нет, backup не создается и DB не пишется;
    - удаление chunked, без `VACUUM`, чтобы не создавать длительный downtime;
    - добавлены `aw-server/aw-db-maintenance.service` и
      `aw-server/aw-db-maintenance.timer`;
    - timer включен на AW server: `OnCalendar=Sun *-*-* 03:30:00`,
      `RandomizedDelaySec=15m`, `Persistent=true`;
    - production dry-run: `planned_delete_rows=0`, `deleted_rows=0`;
    - ручной service smoke: `Result=success`, `ExecMainStatus=0`,
      `planned_delete_rows=0`, `deleted_rows=0`, `backup_created=false`;
    - production timer active; ближайший запуск по remote `systemctl
      list-timers`: `Sun 2026-06-07 03:40:05 UTC`;
    - gates: `cargo fmt --all -- --check`, `cargo test -p aw-db-maintenance`
      `3 passed`, `cargo clippy -p aw-db-maintenance --all-targets -- -D
      warnings`, release build OK, artifact check OK, `quality-gate.sh` OK,
      `ansible-playbook deploy_aw_server.yml --syntax-check` OK;
    - final production gates: AW failed units `0`, `aw-db-health` OK
      (`sqlite.db=359.6MiB`, WAL `4.0MiB`, session rows `174`, recent process
      events `0`, latest `eventType=logon`), DetMir status OK with
      `dlp_counts={ok:22,warn:0,fail:0}`, `ok_for_operator=true`.
36. `[done]` Устранить blocker полного AW server deploy на Influx token:
    - проблема: `deploy_aw_server.yml` падал на assert
      `aw_worktime_influx_enabled=true`, потому что локальные env
      `AW_WORKTIME_INFLUX_TOKEN` и `AW_DLP_INFLUX_TOKEN` пустые;
    - runtime `/etc/activitywatch/aw-server.env` на AW server уже содержал оба
      token, поэтому правильный путь - сохранить remote secrets, а не
      отключать exporters;
    - playbook теперь через `slurp` с `no_log: true` читает текущий
      `/etc/activitywatch/aw-server.env`, выбирает effective token:
      local env token если задан, иначе existing remote token;
    - asserts проверяют effective tokens и не печатают секреты;
    - запись `/etc/activitywatch/aw-server.env` использует effective tokens,
      чтобы full deploy не затирал рабочие Influx credentials пустыми
      значениями;
    - полный `ansible-playbook -i inventory.ini deploy_aw_server.yml
      -e aw_rust_release_dir=<OPERATOR_HOME>/.cache/detmir-adk-rust-target/release`
      прошел до конца: `failed=0`, `ok=282`;
    - final gates после deploy зеленые: `aw-db-health` OK,
      `aw-health-check` OK, `dlp-health-check` `22/0/0`,
      `detmir-check` OK, `detmir-auto --no-heal` OK,
      `detmir-status` severity OK, failed units на AW/Proxmox `0`;
    - Telegram bot/watchdog оставлены `inactive`.
37. `[done]` Закрепить safe default для Windows `processEventsEnabled`:
    - исправлены Windows deploy/recovery defaults, чтобы новый deploy,
      ensemble/domain deploy, hardening recovery и общий
      `New-ActivityWatchDeploymentConfig` не включали process-level
      `aw-session-events` без явного параметра;
    - изменены `windows/deploy-single-user.ps1`,
      `windows/deploy-ensemble.ps1`, `windows/deploy-domain-users.ps1`,
      `windows/hardening-recovery.ps1`,
      `windows/ActivityWatch.Windows.Common.psm1` и
      `windows/validate-deployment.ps1`;
    - синхронизированы текстовые копии в
      `install-kit-awindows-20260427-211240/windows/`;
    - invariant: existing live config сохраняется, но missing/new config
      default всегда `sessionEvents.processEventsEnabled=false`; включать
      можно только явно и временно для forensic/debug окна;
    - local gates: PowerShell AST parse для измененных scripts/modules OK,
      `ansible-playbook -i inventory.ini deploy_aw_windows.yml --syntax-check`
      OK;
    - production deploy: обновленные PowerShell файлы скопированы в
      `C:\Program Files\AWatch-rus\windows` с UTF-8 BOM для Windows
      PowerShell 5.1; remote backup перед заменой:
      `C:\ProgramData\AWatch-rus\switch-backups\windows-safe-default-20260602T091213Z`;
    - `hardening-recovery.ps1` выполнен по existing
      `C:\ProgramData\AWatch-rus\deployment-config.json`; после recovery
      config остался `processEventsEnabled=false`, `logonEnabled=true`,
      `pollSeconds=5`, users включают `HOST-EXAMPLE\Администратор`;
    - exact task check: `ActivityWatch Launch [HOST-EXAMPLE_Администратор]`
      существует, ошибочный
      `ActivityWatch Launch [HOST-EXAMPLE_Administrator]` отсутствует,
      `ActivityWatch Recovery` существует;
    - `AWatchRusCollectorGuard` running/automatic; Windows
      `validate-deployment.ps1` вернул `overallOk=True`;
    - production AW DB guard: session rows `174`, recent process events `0`,
      latest `eventType=logon`, DB около `353MiB`;
    - final gates зеленые: `check-aw-data` buckets fresh/event-driven OK,
      `aw-health-check` OK, `dlp-health-check` `22/0/0`,
      `detmir-check` bucket_ok `8`, stale `0`, dead `0`,
      `detmir-auto --no-heal` OK, `detmir-status` severity OK и
      `ok_for_operator=true`, failed units на AW/Proxmox `0`;
    - краткий transient после recovery: один прогон `detmir-auto` увидел
      `file-operations sendFailuresDelta`; повтор через 30 секунд показал
      `sendFailuresDelta=0`, поэтому коррекция collectors не потребовалась;
    - Telegram bot/watchdog оставлены `inactive` по принятому решению.
38. `[done]` Пересобрать и проверить Windows install kit:
    - выполнен штатный `./scripts/rebuild_install_kit.sh`;
    - пересобраны:
      `install-kit-awindows-20260427-211240/`,
      `install-kit-awindows-20260427-211240.zip`,
      `install-kit-awindows-20260427-211240.tar.gz` и `MANIFEST.txt`;
    - `./scripts/check_install_kit_vs_repo.sh`: compared `62`,
      missing `0`, mismatched `0`, PowerShell mismatches `0`;
    - manifest checksum verification: OK, entries `66`;
    - archives verification: zip и tar.gz распакованы во временные каталоги,
      опасные defaults не найдены; подтверждены
      `[bool]$ProcessEventsEnabled = $false`,
      `effectiveProcessEventsEnabled ... else { $false }`,
      `sessionProcessEventsEnabled ... else { $false }`,
      `aw_windows_process_events_enabled: false`;
    - PowerShell AST parse для kit scripts/modules OK;
    - `ansible-playbook -i inventory.example.ini deploy_aw_windows.yml
      --syntax-check` из install-kit ansible каталога OK.
39. `[done]` Перенести install-kit consistency checker на Rust:
    - добавлен crate `check-install-kit-vs-repo`;
    - `scripts/check_install_kit_vs_repo.sh` теперь Rust-first wrapper:
      ищет `CHECK_INSTALL_KIT_VS_REPO_RUST`,
      `$CARGO_TARGET_DIR/release/check-install-kit-vs-repo`,
      `adk-rust/target/release/check-install-kit-vs-repo`,
      `/usr/local/bin/check-install-kit-vs-repo`, затем использует Python
      fallback;
    - сохранен текстовый output contract:
      `Compared files`, `Missing in repo`, `Mismatched content`,
      `PowerShell mismatches`; добавлен `--json`;
    - поведение усилено: missing/mismatch теперь дают non-zero exit code,
      чтобы рассинхрон install-kit не проходил молча;
    - `scripts/check_detmir_rust_release_artifacts.sh` теперь требует
      `check-install-kit-vs-repo`;
    - после обновления wrapper выполнен `./scripts/rebuild_install_kit.sh`,
      чтобы install kit нес актуальный Rust-first wrapper;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      check-install-kit-vs-repo` (`3 passed`), `cargo clippy -p
      check-install-kit-vs-repo --all-targets -- -D warnings`, release build
      OK, `./scripts/check_install_kit_vs_repo.sh` OK, negative mismatch smoke
      rc `1`, `./scripts/validate_install_kit.sh` OK, artifact check OK.
40. `[done]` Перенести install-kit validator на Rust:
    - добавлен crate `validate-install-kit`;
    - `scripts/validate_install_kit.sh` теперь Rust-first wrapper:
      ищет `VALIDATE_INSTALL_KIT_RUST`,
      `$CARGO_TARGET_DIR/release/validate-install-kit`,
      `adk-rust/target/release/validate-install-kit`,
      `/usr/local/bin/validate-install-kit`, затем использует legacy
      Bash/Python fallback;
    - сохранен stage output:
      `[1/4] Required files presence`,
      `[2/4] Manifest checksum verification`,
      `[3/4] Manifest completeness`,
      `[4/4] Archive composition check`,
      `validate_install_kit: OK`; добавлен `--json`;
    - Rust validator проверяет required files, SHA256 из `MANIFEST.txt`,
      полноту manifest относительно install-kit directory, совпадение состава
      zip/tar.gz и корректный archive prefix;
    - `scripts/rebuild_install_kit.sh` теперь включает
      `scripts/validate_install_kit.sh` в install-kit `scripts/`;
    - `scripts/check_detmir_rust_release_artifacts.sh` теперь требует
      `validate-install-kit`;
    - после wrapper change выполнен `./scripts/rebuild_install_kit.sh`;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      validate-install-kit` (`4 passed`), `cargo clippy -p
      validate-install-kit --all-targets -- -D warnings`, release build OK,
      Rust-first `./scripts/validate_install_kit.sh` OK
      (`MANIFEST complete: 67 files tracked`, `Archives match: 68 files`),
      `./scripts/check_install_kit_vs_repo.sh` OK (`compared 62`,
      mismatches `0`), negative manifest checksum smoke rc `1`, negative
      archive mismatch smoke rc `1`, artifact check OK.
41. `[done]` Перенести InnoSetup installer verifier на Rust:
    - добавлен crate `verify-innosetup-installer`;
    - `scripts/verify_innosetup_installer.sh` теперь Rust-first wrapper:
      ищет `VERIFY_INNOSETUP_INSTALLER_RUST`,
      `$CARGO_TARGET_DIR/release/verify-innosetup-installer`,
      `adk-rust/target/release/verify-innosetup-installer`,
      `/usr/local/bin/verify-innosetup-installer`, затем использует legacy
      Wine/Bash fallback;
    - Rust verifier создает чистый Wine prefix, silent-install'ит
      `windows/installkit/innosetup/AWatch-rus-InstallKit.exe` в
      `C:\AWatchRusExtract`, сверяет payload guard/policy файлов с
      репозиторием и проверяет self-test marker
      `collector guard self-test OK`;
    - первый прогон корректно поймал устаревший installer payload:
      `aw-collector-guard.ps1` и `install-collector-guard-service.ps1`
      отличались от repo после safe-default hardening;
    - installer пересобран штатным `windows/installkit/innosetup/build_with_wine.sh`,
      после чего Rust-first verifier завершился `verify_innosetup_installer:
      OK`;
    - `scripts/check_detmir_rust_release_artifacts.sh` теперь требует
      `verify-innosetup-installer`.
42. `[done]` Перенести install-kit rebuild path на Rust:
    - добавлен crate `rebuild-install-kit`;
    - `scripts/rebuild_install_kit.sh` теперь Rust-first wrapper:
      ищет `REBUILD_INSTALL_KIT_RUST`,
      `$CARGO_TARGET_DIR/release/rebuild-install-kit`,
      `adk-rust/target/release/rebuild-install-kit`,
      `/usr/local/bin/rebuild-install-kit`, затем использует legacy Bash
      fallback;
    - Rust rebuild сохраняет `*.deployment-config.json`, пересобирает
      directory tree, `MANIFEST.txt`, `.zip` и `.tar.gz`;
    - копирование реализовано через read + replace-write, а не `fs::copy`,
      потому что текущий mounted-диск возвращал `Operation not permitted` на
      truncate/copy existing files;
    - install-kit теперь включает сам `scripts/rebuild_install_kit.sh`, чтобы
      комплект был самодостаточным для дальнейшей пересборки;
    - Rust-first rebuild verified через `validate_install_kit`,
      `check_install_kit_vs_repo`, archive script `cmp`, and release artifact
      check.
43. `[done]` Перенести `quality-gate.sh` на Rust-first orchestrator:
    - добавлен crate `quality-gate`;
    - `scripts/quality-gate.sh` теперь Rust-first wrapper:
      ищет `QUALITY_GATE_RUST`,
      `$CARGO_TARGET_DIR/release/quality-gate`,
      `adk-rust/target/release/quality-gate`, `/usr/local/bin/quality-gate`,
      затем использует legacy Bash fallback;
    - Rust orchestrator сохраняет stage contract: Bash syntax,
      ShellCheck when available, Node syntax when available, PowerShell parse
      and collector guard self-test when `pwsh` available, Ansible syntax when
      `ansible-playbook` available;
    - final gates: `cargo fmt --all -- --check`, targeted tests for
      `verify-innosetup-installer`, `rebuild-install-kit`, `quality-gate`,
      `validate-install-kit`, clippy `-D warnings`, release builds,
      Rust-first `quality-gate: OK`, install-kit validate/check OK,
      InnoSetup verifier OK, and DetMir read-only status OK with
      `ok_for_operator=true`.
44. `[done]` Перенести offline Sigma/Hayabusa IOC extraction path на Rust:
    - добавлен crate `extract-ioc-from-sigma`;
    - `scripts/build_dlp_ioc_from_hayabusa.sh` теперь Rust-first wrapper:
      ищет `EXTRACT_IOC_FROM_SIGMA_RUST`,
      `$CARGO_TARGET_DIR/release/extract-ioc-from-sigma`,
      `adk-rust/target/release/extract-ioc-from-sigma`,
      `/usr/local/bin/extract-ioc-from-sigma`, затем использует legacy Python
      `scripts/extract_ioc_from_sigma.py`;
    - Rust binary сохраняет CLI-контракт `--rules-root`, `--out-dir`,
      `--table-name` и output files `ioc_blacklist.json`,
      `ioc_blacklist.csv`, `ioc_blacklist.sql`;
    - extraction contract сохранен: `Image|endswith`,
      `CommandLine|contains`, `OriginalFileName`, `Hashes|SHA256` и SHA256
      values embedded in `Hashes` strings;
    - parity fix: Rust использует Unicode lowercase и PyYAML-compatible
      `True`/`False` rendering для YAML 1.1 boolean-like literals, чтобы не
      расширять IOC set относительно legacy Python;
    - real Hayabusa rules parity:
      `rules_scanned=4963`, `iocs_extracted=13510`,
      `commandline_contains=10192`, `original_filename=457`,
      `process_image_endswith=1171`, `sha256=1690`,
      `missing_in_rust=0`, `extra_in_rust=0`;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      extract-ioc-from-sigma` (`3 passed`), `cargo clippy -p
      extract-ioc-from-sigma --all-targets -- -D warnings`, release build OK,
      `scripts/check_detmir_rust_release_artifacts.sh` OK,
      Rust-first `scripts/quality-gate.sh` OK.
45. `[done]` Перенести `scripts/rdp-worktime-report.sh` на Rust-first helper:
    - добавлен crate `rdp-worktime-report`;
    - `scripts/rdp-worktime-report.sh` теперь Rust-first wrapper:
      ищет `RDP_WORKTIME_REPORT_RUST`,
      `$CARGO_TARGET_DIR/release/rdp-worktime-report`,
      `adk-rust/target/release/rdp-worktime-report`,
      `/usr/local/bin/rdp-worktime-report`, затем использует legacy embedded
      Python fallback;
    - Rust binary сохраняет CLI/env contract: `--day today|yesterday`,
      `--from YYYY-MM-DD --to YYYY-MM-DD`, `AW_BASE_URL`,
      `AW_WORKTIME_HOST`, `AW_WORKTIME_DEFAULT_SAMPLE_SECONDS`,
      `AW_WORKTIME_MAX_SAMPLE_SECONDS`, `OUT_DIR`;
    - output contract сохранен: `rdp-worktime-<FROM>_<TO>.csv`,
      `rdp-worktime-<FROM>_<TO>.json`, stdout paths plus `CSV:`/`JSON:`;
    - calculation contract сохранен: bucket
      `aw-worktime-sessions_<host>`, active samples by `active=true` or
      Russian/English active state, interval merge, sampleSeconds/pollSeconds,
      duration and next-timestamp fallback, canonical `HOST\user` userId;
    - live parity on yesterday against Python fallback:
      `rust_csv_rows=3`, `python_csv_rows=3`, `csv_equal=True`,
      JSON `host/bucket_id/from/to/rows` all equal;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      rdp-worktime-report` (`3 passed`), `cargo clippy -p
      rdp-worktime-report --all-targets -- -D warnings`, release build OK,
      `scripts/check_detmir_rust_release_artifacts.sh` OK,
      `scripts/quality-gate.sh` OK, DetMir read-only status OK with
      `dlp_counts={ok:22,warn:0,fail:0}` and `ok_for_operator=true`.
46. `[done]` Перенести Proxmox DetMir contour smoke на Rust-first helper:
    - добавлен crate `aw-contour-smoke`;
    - `scripts/aw-contour-smoke-gateway.sh` теперь Rust-first wrapper:
      ищет `AW_CONTOUR_SMOKE_RUST`,
      `$CARGO_TARGET_DIR/release/aw-contour-smoke`,
      `adk-rust/target/release/aw-contour-smoke`,
      `/usr/local/sbin/aw-contour-smoke`,
      `/usr/local/bin/aw-contour-smoke`, затем использует legacy Bash
      fallback;
    - `scripts/aw-contour-smoke-local.sh` умеет временно доставлять release
      binary на Proxmox для shadow smoke без постоянного platform change;
    - remote parity: Rust и legacy оба вернули `OK=30 WARN=0 FAIL=3 SKIP=0`,
      `rc=2`; все 3 fail совпали и относятся к существующему gateway-auth
      `HTTP 401` на `/go/proxmox-gui`, `/go/file1c-brief`,
      `/go/file1c-actions`, не к Rust regression;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      aw-contour-smoke` (`2 passed`), `cargo clippy -p aw-contour-smoke
      --all-targets -- -D warnings`, release build OK,
      `bash -n` wrappers OK, artifact check OK, `quality-gate.sh` OK.
47. `[done]` Перенести `scripts/diag_and_manual_restart.sh` на Rust-first
    diagnostic helper:
    - добавлен crate `diag-and-manual-restart`;
    - wrapper ищет `DIAG_AND_MANUAL_RESTART_RUST`,
      `$CARGO_TARGET_DIR/release/diag-and-manual-restart`,
      `adk-rust/target/release/diag-and-manual-restart`,
      `/usr/local/bin/diag-and-manual-restart`, затем использует legacy Bash
      fallback;
    - здоровый контур не перезапускается: helper запускает `aw-health-check`
      и `dlp-health-check`, печатает `Diagnostics: healthy. Restart not
      needed.` и выходит `0`;
    - mutation path требует явного подтверждения или `--yes`; live restart не
      запускался, потому что контур был зеленый;
    - gates: fmt/test/clippy/release OK, `bash -n` OK, Rust-first healthy path
      rc `0`, legacy fallback healthy path rc `0`, artifact check OK,
      `quality-gate.sh` OK, DetMir read-only OK.
48. `[done]` Перенести browser smoke launcher на Rust-first helper:
    - добавлен crate `aw-browser-smoke`;
    - `scripts/aw-webui-browser-smoke.sh` теперь Rust-first wrapper:
      ищет `AW_BROWSER_SMOKE_RUST`,
      `$CARGO_TARGET_DIR/release/aw-browser-smoke`,
      `adk-rust/target/release/aw-browser-smoke`,
      `/usr/local/bin/aw-browser-smoke`, затем использует legacy Node
      fallback;
    - Playwright/Chromium логика остается в
      `scripts/aw-webui-browser-smoke.mjs`; Rust отвечает за устойчивый запуск,
      `NODE_PATH`, passthrough args и сохранение child exit code;
    - fake-node проверки подтвердили passthrough `.mjs` path, user args,
      `NODE_PATH` и exit-code propagation для Rust launcher и legacy fallback;
    - live browser smoke через Rust-first wrapper: `ok=true`, страницы
      `aw_webui_home`, `worktime_today_html`,
      `worktime_management_html` все OK;
    - gates: `cargo fmt --all -- --check`, `cargo test -p aw-browser-smoke`
      (`3 passed`), `cargo clippy -p aw-browser-smoke --all-targets -- -D
      warnings`, release build OK, `bash -n` OK, artifact check OK,
      `quality-gate.sh` OK, DetMir read-only OK with
      `dlp_counts={ok:22,warn:0,fail:0}` and `ok_for_operator=true`.
49. `[done]` Перенести `check-aw-full.sh` на Rust-first read-only helper:
    - добавлен crate `check-aw-full`;
    - root wrapper `check-aw-full.sh` ищет `CHECK_AW_FULL_RUST`,
      `$CARGO_TARGET_DIR/release/check-aw-full`,
      `adk-rust/target/release/check-aw-full`, `/usr/local/bin/check-aw-full`,
      затем выполняет embedded legacy Bash fallback;
    - для прямого legacy compare добавлен `CHECK_AW_FULL_FORCE_LEGACY=1`;
    - Rust helper сохраняет операторский контракт: AW server connectivity,
      CORS probe, 8 bucket freshness rows, RDP WinRM/SSH TCP checks, summary
      `FRESH/STALE/DEAD`, recovery hint when stale/dead collectors exist;
    - live parity: Rust-first и legacy оба показали connectivity OK, CORS OK,
      WinRM OK, SSH OK, `FRESH=8 STALE=0 DEAD=0`; bucket statuses совпали,
      один live event id у `Window watcher` ожидаемо сдвинулся между
      последовательными запусками;
    - gates: `cargo fmt --all -- --check`, `cargo test -p check-aw-full`
      (`4 passed`), `cargo clippy -p check-aw-full --all-targets -- -D
      warnings`, release build OK, `bash -n` OK, artifact check OK,
      `quality-gate.sh` OK, DetMir read-only OK with
      `dlp_counts={ok:22,warn:0,fail:0}` and `ok_for_operator=true`.
50. `[done]` Перенести merge engine на Rust:
    - добавлен crate `merge-aw-server-dbs`;
    - рабочий runtime path: `adk-rust/target/release/merge-aw-server-dbs` и
      `/usr/local/bin/merge-aw-server-dbs`;
    - legacy `scripts/merge_aw_server_dbs.py` удален из repo/runtime path;
    - `ansible/deploy_aw_server.yml` устанавливает
      `/usr/local/bin/merge-aw-server-dbs` и использует его без Python;
    - Rust сохраняет контракт `--base`, `--overlay`, `--output`, SQLite
      backup-copy base DB, bucket matching by id/key/name, duplicate event
      suppression by `(starttime,endtime,data)`, JSON summary;
    - offline parity fixture: Rust replacement дал ожидаемый JSON
      `inserted_buckets=1`, `inserted_events=3` и одинаковые SQLite table rows
      (`buckets=3`, `events=5`) на кейсе duplicate event + name-conflict
      bucket;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      merge-aw-server-dbs` (`2 passed`), `cargo clippy -p
      merge-aw-server-dbs --all-targets -- -D warnings`, release build OK,
      artifact check OK, `quality-gate.sh` OK, DetMir read-only OK after DLP baseline retry
      with `dlp_counts={ok:22,warn:0,fail:0}` and `ok_for_operator=true`.
51. `[done]` Перенести `scripts/prod_backup_restore.sh` в safe-by-default
    Rust planner/checker:
    - добавлен crate `prod-backup-restore`;
    - `scripts/prod_backup_restore.sh` теперь Rust-first wrapper:
      ищет `PROD_BACKUP_RESTORE_RUST`,
      `$CARGO_TARGET_DIR/release/prod-backup-restore`,
      `adk-rust/target/release/prod-backup-restore`,
      `/usr/local/bin/prod-backup-restore`;
    - обычный запуск больше не выполняет restore при наличии Rust artifact:
      он печатает plan-only flow и помечает destructive steps;
    - если Rust artifact отсутствует, wrapper отказывается запускать
      destructive legacy случайно и просит собрать Rust planner;
    - старый Python destructive flow удален; `--apply-legacy` больше не
      поддерживается;
    - Rust `--apply` намеренно запрещен на этом этапе;
    - `--check-inputs --json` валидирует env/files/commands без вывода
      секретов, проверяет `AW_SSH_PASSWORD`, `AW_WINRM_PASSWORD`, `sshpass`,
      `ansible-playbook`, inventory и `merge-aw-server-dbs`;
    - план сохраняет порядок restore flow: scp Rust merge binary, remote backup dir,
      DB existence checks, DB backups, stop `activitywatch-server`, merge DB,
      install merged DB, 3 ansible playbooks, final AW validation;
    - safe smoke: plan text OK, JSON valid (`steps=12`, `missing_count=0`),
      destructive steps present and marked, password names/values не попадают в
      planned commands, Rust `--apply` exits rc `1`;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      prod-backup-restore` (`3 passed`), `cargo clippy -p
      prod-backup-restore --all-targets -- -D warnings`, release build OK,
      `bash -n scripts/prod_backup_restore.sh`, artifact check OK,
      `quality-gate.sh` OK, DetMir read-only OK with
      `dlp_counts={ok:22,warn:0,fail:0}` and `ok_for_operator=true`;
      transient `service_warnings=1` disappeared on immediate
      `detmir-check --json` repeat (`service_warnings=0`).
52. `[done]` Перенести `scripts/prod_rollout.sh` в Rust-first safe
    planner/orchestrator:
    - добавлен crate `prod-rollout`;
    - `scripts/prod_rollout.sh` теперь Rust-first wrapper:
      ищет `PROD_ROLLOUT_RUST`, `$CARGO_TARGET_DIR/release/prod-rollout`,
      `adk-rust/target/release/prod-rollout`, `/usr/local/bin/prod-rollout`;
    - обычный запуск больше не стартует production deploy автоматически:
      он показывает plan-only rollout flow;
    - реальный rollout через Rust доступен только явно:
      `scripts/prod_rollout.sh --apply`;
    - старый Bash rollout сохранен только явно:
      `scripts/prod_rollout.sh --apply-legacy`;
    - `--check-inputs --json` валидирует `AW_SSH_PASSWORD`,
      `AW_WINRM_PASSWORD`, `git`, `ansible`, `ansible-playbook`, inventory,
      `quality-gate.sh`, `deploy_aw_server.yml`, `deploy_aw_windows.yml` и
      `post_validate_aw_windows.yml` без вывода секретов;
    - план сохраняет legacy порядок: `quality-gate`, Ansible ping AW server,
      Ansible win_ping AW Windows, `deploy_aw_server --check --diff`, real
      `deploy_aw_server`, `deploy_aw_windows --check --diff`, real
      `deploy_aw_windows`, `post_validate_aw_windows`;
    - mutation в плане помечена только для `deploy-aw-server` и
      `deploy-aw-windows`;
    - apply-runner пишет stdout/stderr каждого шага в
      `.rollout-logs/<timestamp>/`, общий ход - в `rollout.log`;
    - safe smoke: `scripts/prod_rollout.sh --check-inputs --json` вернул
      `mode=plan-only`, `missing_count=0`, `steps=8`, mutations
      `deploy-aw-server` и `deploy-aw-windows`;
    - gates: `cargo fmt --all -- --check`, `cargo test -p prod-rollout`
      (`3 passed`), `cargo clippy -p prod-rollout --all-targets -- -D
      warnings`, release build OK, `bash -n scripts/prod_rollout.sh`,
      artifact check OK.
53. `[done]` Перенести `aw-server/ensure-reliability.sh` в Rust-first
    dry-run/apply helper:
    - добавлен crate `aw-ensure-reliability`;
    - `aw-server/ensure-reliability.sh` теперь Rust-first wrapper:
      ищет `AW_ENSURE_RELIABILITY_RUST`,
      `$CARGO_TARGET_DIR/release/aw-ensure-reliability`,
      `adk-rust/target/release/aw-ensure-reliability`,
      `/usr/local/bin/aw-ensure-reliability`;
    - обычный запуск больше не делает `chown`, `systemctl stop/start`,
      logrotate write или health timer write; он показывает dry-run plan;
    - реальный Rust repair требует explicit `--apply`;
    - старый Bash repair доступен только explicit `--apply-legacy`;
    - Rust helper сохраняет legacy action set: env check,
      ownership/mode repair for `/var/lib/activitywatch`,
      `/var/log/activitywatch`, `/opt/activitywatch`, logrotate install,
      health-check script/timer/service install, ordered AW service restart and
      enable;
    - `deploy_aw_server.yml` optional устанавливает
      `/usr/local/bin/aw-ensure-reliability` до Influx token checks, если
      release artifact доступен;
    - production binary доставлен на AW server, но `--apply` не запускался;
    - production dry-run: `apply=false`, `ok=true`, `missing=0`,
      `executed=0`, `steps=25`;
    - final production gates: AW failed units `0`, DetMir status OK with
      `service_warnings=0`, `dlp_counts={ok:22,warn:0,fail:0}`,
      `ok_for_operator=true`;
    - gates: `cargo fmt --all -- --check`, `cargo test -p
      aw-ensure-reliability` (`2 passed`), `cargo clippy -p
      aw-ensure-reliability --all-targets -- -D warnings`, release build OK,
      `bash -n aw-server/ensure-reliability.sh`, artifact check OK,
      `ansible-playbook deploy_aw_server.yml --syntax-check` OK.
54. `[done]` Перевести Linux install scripts в Rust-first safe planner слой:
    - добавлен crate `aw-linux-install`;
    - wrappers теперь Rust-first и dry-run по умолчанию:
      `scripts/install_aw_linux_client.sh`,
      `scripts/install_aw_linux_remote_worker.sh`,
      `scripts/install_aw_linux_web_category_logger.sh`,
      `scripts/install_aw_console_ssh_logger.sh`,
      `scripts/install_aw_pve_webadmin_logger.sh`;
    - real install требует explicit `--apply`;
    - old shell install требует explicit `--apply-legacy`;
    - `aw-linux-install --apply` запускает соответствующий legacy script с
      `--apply-legacy`, сохраняя embedded Python collectors и shell install
      contract;
    - `remote_worker` legacy path исправлен: вложенные client/console/web
      installers вызываются с `--apply-legacy`, чтобы real legacy install не
      превратился в dry-run после оборачивания;
    - PVE webadmin logger не запускался и Proxmox platform не менялся; это
      только safe planner wrapper;
    - gates: `cargo fmt --all -- --check`, `cargo test -p aw-linux-install`
      (`2 passed`), `cargo clippy -p aw-linux-install --all-targets -- -D
      warnings`, release build OK, `sh -n` для всех пяти wrappers OK,
      dry-run JSON для всех пяти wrappers OK, artifact check OK.
55. `[done]` Закрепить проверку актуализации и правильности Grafana данных:
    - добавлен crate `detmir-grafana-check`;
    - check читает Grafana dashboard API, валидирует форму dashboard,
      отсутствие старых `aw_window_event`/`aw_afk_event`, наличие текущих
      worktime measurements и выполняет все panel queries через Grafana
      datasource API;
    - freshness panel считается обязательным: значение свежести должно быть не
      старше `DETMIR_GRAFANA_MAX_FRESHNESS_MINUTES` (production default 360);
    - результат пишется в Grafana CT 201:
      `/var/lib/detmir-grafana-check/latest.json` и `latest.txt`;
    - установлен `detmir-grafana-check.service` и timer каждые 15 минут;
    - `detmir-check` теперь читает последний Grafana-check artifact через
      `sudo -n /usr/sbin/pct exec 201` и считает `grafana-data` required
      service check, так что общий `detmir-status` краснеет при сломанной или
      устаревшей Grafana;
    - добавлен playbook `ansible/deploy_grafana_check.yml`;
    - `scripts/check_detmir_rust_release_artifacts.sh` теперь требует
      `detmir-grafana-check`;
    - production verification: `detmir-grafana-check` OK
      (`ok=13,warn=0,fail=0`), 7/7 panels have rows, freshness около 120 min,
      timer active, Grafana CT failed units 0, Proxmox failed units 0,
      `detmir-check` `service_failures=0`, `detmir-auto --no-heal` rc 0,
      `detmir-status` OK / `ok_for_operator=true`.
56. `[done]` Реализовать read-only MVP `detmir-portal`:
    - добавлен crate `detmir-portal`;
    - portal работает как Rust web service на Proxmox:
      `/usr/local/bin/detmir-portal`, `detmir-portal.service`,
      bind `127.0.0.1:8720`;
    - внешний route добавлен в существующий nginx gateway:
      `https://<PUBLIC_GATEWAY_FQDN>/portal/`;
    - UI содержит вкладки `Оператор`, `Руководитель`, `Владелец`,
      `Инциденты ИБ`;
    - API реализованы: `/api/health`, `/api/summary`, `/api/operator`,
      `/api/manager`, `/api/owner`, `/api/incidents`, `/api/links`;
    - источники read-only: `detmir-status --json`, `detmir-check --json`,
      `systemctl --failed`, AW Worktime API, 1C analytics health;
    - portal не делает write/heal/restart actions и не трогает pfSense,
      Telegram runtime, NAT/DNS/VPN;
    - добавлен deployment playbook `ansible/deploy_detmir_portal.yml`;
    - gateway playbook теперь проверяет `/portal/api/health` с auth;
    - `scripts/check_detmir_rust_release_artifacts.sh` теперь требует
      `detmir-portal`;
    - gates: `cargo fmt --all -- --check`, `cargo test -p detmir-portal`
      (`3 passed`), `cargo clippy -p detmir-portal --all-targets -- -D
      warnings`, release build OK, `ansible-playbook
      deploy_detmir_portal.yml --syntax-check`, `ansible-playbook
      deploy_proxmox_web_gateway.yml --syntax-check`, artifact check OK;
    - production verification: `detmir-portal` active, nginx active,
      `/portal/api/health` OK through gateway auth, all sources true,
      external `/portal/` returns protected `401` without auth, gateway
      `/healthz` returns `ok`, `detmir-status` OK / `ok_for_operator=true`,
      Proxmox failed units 0;
    - browser verification through production tunnel: desktop and mobile
      nonblank, tabs work, API requests 200, console errors 0. Screenshots:
      `.playwright-cli/page-2026-06-02T17-31-49-049Z.png`,
      `.playwright-cli/page-2026-06-02T17-33-50-318Z.png`,
      `.playwright-cli/page-2026-06-02T17-34-04-725Z.png`.
    - portal link repair after Grafana/gateway smoke: quick links now open in a
      separate tab, `worktime_report` points to explicit HTML
      `/reports/worktime/management?format=html&host=HOST-EXAMPLE`,
      `1С действия` is shown in the portal, and gateway `/r/aw-worktime` is
      pinned to the same HTML report. Production smoke: AW UI, Worktime, 1C
      brief, and 1C actions returned `200 text/html`; Grafana links correctly
      redirect to `/login` when no Grafana session exists.
    - incident operator workflow added to portal: incidents now have stable
      IDs, operator state is persisted in
      `/var/lib/detmir-portal/incidents-state.json`, audit events append to
      `/var/lib/detmir-portal/audit.jsonl`, nginx passes `$remote_user` as
      `X-Remote-User`, and the UI exposes `В работу` / `Назначить` actions.
      This layer only records acknowledgement/assignment metadata; it does not
      heal, restart, mutate AW/DLP/1C, or touch pfSense/Telegram. Gates:
      `cargo fmt --all -- --check`, `cargo test -p detmir-portal` (`5 passed`),
      `cargo clippy -p detmir-portal --all-targets -- -D warnings`, release
      build OK. Production smoke: `POST /portal/api/incidents/action` returned
      `200 application/json`, actor resolved to `detmir`, state/audit files
      were written, smoke state was removed after validation, portal health
      stayed `200`, `detmir-status` stayed `OK / ok_for_operator=true`, and
      failed units stayed `0`. Local Playwright smoke against a temporary
      portal instance verified `Инциденты ИБ`, `В работу`, persisted UI state,
      disabled ack button, and zero JS errors.
    - `Инциденты ИБ` tightened to a DLP-focused card: the tab now filters the
      card to DLP/incident/case items, adds a synthetic DLP-count incident when
      `dlp_counts.warn/fail` is non-zero, and shows only DLP/Grafana dashboard
      links (`detmir-dlp-security`, `detmir-dlp-management`,
      `awatch-dlp-overview`, Grafana catalog). Worktime, 1C, AW UI, and other
      non-incident operational links are intentionally not shown in this card.
      Local Playwright smoke verified DLP incident rendering, dashboard links,
      no Worktime/1C text in the card, and zero JS errors.
    - DLP evidence viewing layer added to the portal:
      `detmir-portal` now exposes `/api/dlp/evidence` and safe screenshot
      routes by opaque evidence id. The service reads DLP warehouse SQLite
      read-only, extracts `screenshotSha256`/dimensions from raw event JSON,
      and serves image files only from an allowlisted evidence root after
      canonical path, extension, size, and SHA-256 validation. Evidence views
      and downloads append to `evidence-audit.jsonl`. Production uses an
      AW-server evidence-only service, `/usr/local/bin/detmir-portal-evidence`
      with `detmir-portal-evidence.service`, because the DLP warehouse lives on
      the AW server. Proxmox nginx gateway routes
      `/portal/api/dlp/evidence*` to `<AW_SERVER_HOST>:8721`. Current production
      verification: AW evidence API `ok=true`, gateway evidence route
      `ok=true`, `db_available=true`, 11 DLP evidence rows returned,
      `screenshot_available=0` because current stored rows do not yet contain
      screenshot metadata, both portal/evidence services active, failed units
      0, `detmir-status` `OK / ok_for_operator=true`. Local HTTP smoke
      verified byte-identical screenshot serving and audit logging; local
      Playwright smoke verified the `Доказательства` block, `СКРИН`, `Открыть`,
      `Скачать`, and zero JS errors.
    - DLP evidence screenshot delivery is now automated in production:
      the AW evidence API accepts authenticated uploads at
      `POST /api/dlp/evidence/upload` in evidence-only mode. Uploads require a
      server-generated Bearer token stored outside git, validate base64 body,
      PNG/JPEG magic, max size, and exact SHA-256 before atomic write to
      `/var/lib/activitywatch/dlp-evidence/screenshots/<sha256>.(png|jpg)`.
      Windows RDP host runs `sync-dlp-evidence-artifacts.ps1` through scheduled
      task `ActivityWatch DLP Evidence Sync` every 5 minutes as SYSTEM. The
      sync scans `C:\ProgramData\AWatch-rus\incident-artifacts` plus configured
      artifact roots, uploads new PNG screenshots, and keeps local upload state
      in `C:\ProgramData\AWatch-rus\dlp-evidence-sync-state.json`. Production
      controlled test: a visible PNG was created on Windows, sync uploaded it
      (`uploaded=1`, `failed=0`), a temporary warehouse row made it visible in
      the portal with `screenshot_available=true`, gateway preview returned
      `image/png` with matching SHA, external Playwright verified the portal UI
      and preview, audit recorded upload/view, unauthenticated upload returned
      403, and the synthetic row/files/state were removed. Final state:
      evidence count returned to 11, synthetic hit false, AW/Proxmox failed
      units 0, `detmir-status` OK, sync task Ready with `lastTaskResult=0`.
    - during this deploy, `detmir-grafana-check` was corrected so empty
      detail-only panels for employees/applications are WARN, not FAIL. The
      mandatory freshness/summary panels still fail the check when stale or
      empty. This removed a false red state shortly after midnight when
      `today` detail rows were legitimately empty. Production verification:
      Grafana check `ok=true` with `fail=0`, `detmir-auto --no-heal` rc `0`,
      portal health `true`, `detmir-status` `OK / ok_for_operator=true`, and
      failed units `0`.
    - `docs/DETMIR_THREAT_MODEL_RU.md` added as the current working threat
      model for DetMir. It records the product as an operational
      control and technical audit platform, not a certified DLP/SIEM/EDR/XDR
      or FSTEC SZI. It also records Igor as the declared product owner, lists
      assets, trust zones, attacker/operator-failure classes, implemented
      evidence controls, residual risks, and the hardening roadmap.
    - `docs/DETMIR_RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md` added as the
      registry/product positioning note. Current decision: lead with DetMir as
      an operational control and IT infrastructure management platform, use
      `09.10` as the primary Russian software registry class target, keep
      DLP/security/evidence/Hayabusa as applied modules, and prepare website,
      operator/admin docs, ownership package, screenshots, and dependency
      inventory before any registry filing.
    - registry proof package skeleton added:
      `docs/ADMIN_GUIDE_RU.md`, `docs/OPERATOR_GUIDE_RU.md`,
      `docs/INSTALL_RU.md`, `docs/ARCHITECTURE_RU.md`,
      `docs/OWNERSHIP_RU.md`, `docs/THIRD_PARTY_LICENSES_RU.md`, and
      `docs/REGISTRY_CHECKLIST_RU.md`. Naming decision fixed across the docs:
      `DetMir` is the product, `AWatch-rus` is the repository/technical base,
      and the external formula is `DetMir, программный комплекс на базе
      AWatch-rus`.

Отложить:

- post-MVP развитие `detmir-portal`: role-aware views, safe check-now action,
  daily owner report, historical trends, AI summary with strict source
  citations, action buttons with allowlist and audit log. Детальный план:
  `docs/DETMIR_PORTAL_GUI_PLAN_RU.md`;
- перенос Telegram bot runtime снят с плана: Python остается постоянным
  runtime, Rust используется только для backend helpers;
- перенос оставшихся install/runtime scripts на Rust;
- переписывание PowerShell Windows collector path;
- любые network/firewall/VPN изменения;
- pfSense полностью frozen/no-touch: текущего потенциала достаточно, NAT/gateway
  tooling не переносить и не дергать без отдельной явной команды.

## 12. Stop conditions

Миграцию остановить и откатить конкретный модуль, если:

- новый binary дает иной `severity` без объяснимой причины;
- меняет exit code class на зеленом или красном контуре;
- пишет неполный/corrupt state;
- вызывает restart на зеленом контуре;
- ломает Telegram `/status`;
- ломает Grafana/Influx/Prometheus metric names;
- требует laptop-only path или интерактивный shell;
- выводит секреты в stdout/journald/report.

## 13. Рабочий принцип

Правильный перенос на Rust - это не переписывание строк один-в-один.

Для каждого legacy script надо сделать:

1. понять контракт;
2. зафиксировать fixture;
3. написать typed model;
4. реализовать read-only parity;
5. включить shadow-mode;
6. заменить production command;
7. только потом добавлять action/mutation;
8. удалить legacy только после периода стабильной эксплуатации.
