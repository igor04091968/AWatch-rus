# Roadmap: замена PowerShell на Rust EXE

Дата: 2026-06-05

Цель: убрать зависимость AWatch-rus от PowerShell-скриптов на рабочих
хостах и заменить их на самодостаточные Rust EXE/службы без потери данных,
без ухудшения пилотной демонстрации и без изменения функционального объема.

Документ описывает порядок миграции. Он не вводит новые функции: каждая Rust
замена сначала должна повторить текущий контракт PowerShell-компонента.

Актуальная статусная матрица оставшихся PowerShell-файлов:
[`POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md`](POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md).

Операторский checkpoint от 2026-06-05 сохранен в private `.ops`-контуре:
Phase 0 live inventory выполнен, Phase 2 validation для уже переведенных
Windows Rust paths выполнен успешно. В tracked документации не публикуются live
hostnames, private IP, токены и runtime evidence paths.

## 1. Целевое состояние

К концу миграции:

- на Windows/RDP-хостах регулярный сбор данных, evidence-sync, 1C upload,
  guard/recovery и validation выполняются Rust EXE;
- в Scheduled Tasks и Windows Services нет штатных AWatch-rus задач, которые
  запускают `powershell.exe` или `pwsh.exe`;
- install-kit поставляет Rust EXE, конфиги и service/task definitions вместо
  runtime `.ps1`;
- Ansible разворачивает Rust binaries и конфиги, а не копирует PowerShell
  runtime scripts;
- Telegram/Codex/bot контур вызывает Rust helpers или читает Rust JSON
  snapshots, не опираясь на пути к `.ps1`;
- legacy PowerShell хранится только как временный rollback-слой до прохождения
  acceptance gates и затем удаляется из install-kit;
- Linux/AW/Proxmox части остаются Rust-first; Ansible может остаться
  оркестратором, потому что это инфраструктурный deploy layer, а не runtime
  PowerShell.

## 2. Текущее состояние

Уже выполнено или частично выполнено:

- `awatch-agent-rs` заменяет Windows `worktime-session-collector.ps1` для
  worktime/RDP path. Legacy PowerShell оставлен как fallback и управляется
  `collectors.worktimeSessionEnabled`, `worktimeSessionMode`,
  `worktimeLegacyFallbackEnabled`.
- `aw-windows-telemetry.exe` уже используется для:
  - `file1c-upload`;
  - `dlp-evidence-sync`.
- `aw-1c-ingest-rust` пишет 1C/file analytics в ClickHouse на серверной
  стороне.
- `aw-windows-telemetry.exe validate-deployment` добавлен как Rust validation
  gate первого уровня. Он проверяет уже мигрированные Windows Rust paths,
  свежесть worktime bucket, Rust collector guard service и queue sanity без
  запуска PowerShell.
- `AWatchRusCollectorGuard` переключен с `aw-collector-guard.ps1` на
  `aw-windows-telemetry.exe collector-guard`. Старый PowerShell guard оставлен
  как rollback script, но штатный service runtime его не запускает. Rust guard
  получает `sessionId` из native process snapshot, дедуплицирует legacy
  collectors по `(kind, sessionId)` и не запускает пользовательские launch
  tasks повторно, если legacy collectors уже активны.
- `aw-windows-telemetry.exe` добавил P0 runtime collector subcommands:
  `browser-domains-collector`, `dlp-endpoint-collector`,
  `file-operations-collector`. На live Windows/RDP host они включены через
  `collectors.*Mode=rust_primary`, работают в трех пользовательских сессиях,
  а P0 PowerShell collector runtime отсутствует. Это не означает полного
  удаления legacy `.ps1`: они оставлены как rollback/reference до расширения
  глубокой функциональной parity.
- Основные серверные AWatch-rus/AW/DLP helpers уже Rust-first:
  `detmir-status`, `detmir-check`, `detmir-auto`, `detmir-heal-safe`,
  `aw-rus-healthd-rust`, `dlp-*`, `worktime-*`, `aw-health-check`,
  `check-aw-data`, `aw-prune-local-state` и другие.

Остаток PowerShell в рабочем дереве:

- 26 product `.ps1` в `windows/`;
- operator/MCP helper `scripts/powershell/detmir-powershell-profile.ps1`;
- parse-check helper `.pssa_run.ps1`;
- `.venv/bin/activate.ps1` внутри virtualenv не является продуктовым
  компонентом и не входит в миграцию.

## 3. Инвентаризация Windows PowerShell

| Скрипт | Роль | Цель миграции | Приоритет |
|---|---|---|---|
| `worktime-session-collector.ps1` | RDP/worktime сбор | закрепить замену через `awatch-agent-rs`, затем удалить fallback | P0 done/stabilize |
| `browser-domains-native-collector.ps1` | browser/domain и DLP web-сигналы | `aw-windows-telemetry.exe browser-domains-collector`; live Rust primary, legacy fallback/reference | P0 live/stabilize |
| `dlp-endpoint-signals-collector.ps1` | clipboard/USB/print/DLP incident signals | `aw-windows-telemetry.exe dlp-endpoint-collector`; live Rust primary, screenshots только для DLP events | P0 live/stabilize |
| `file-operations-collector.ps1` | file create/delete/rename/archive hints | `aw-windows-telemetry.exe file-operations-collector`; live Rust primary with queue/spool | P0 live/stabilize |
| `email-outbound-collector.ps1` | outbound email metadata/DLP | Rust email metadata collector | P1 |
| `dlp-policy-client.ps1` | получение DLP policy | Rust policy client/cache | P1 |
| `export-upload-file-1c-telemetry.ps1` | 1C telemetry upload | закрепить `aw-windows-telemetry.exe file1c-upload`, затем удалить legacy | P0 done/stabilize |
| `sync-dlp-evidence-artifacts.ps1` | DLP evidence upload | закрепить `aw-windows-telemetry.exe dlp-evidence-sync` | P0 done/stabilize |
| `export-evtx-for-hayabusa.ps1` | bounded EVTX export | Rust EVTX export helper | P1 |
| `export-upload-hayabusa-to-aw-server.ps1` | EVTX/Hayabusa upload | Rust upload helper или режим в `aw-windows-telemetry.exe` | P1 |
| `aw-standalone-service.ps1` | supervisor service wrapper | Rust Windows service wrapper | P0 |
| `aw-collector-guard.ps1` | guard/restart/recovery | Rust guard with allowlist, lock, cooldown | P0 done/stabilize |
| `hardening-recovery.ps1` | recovery/hardening | Rust recovery CLI; destructive actions behind `--apply` | P1 |
| `validate-deployment.ps1` | post-deploy validation | Rust validation CLI `aw-windows-telemetry.exe validate-deployment`; расширять до полной parity перед удалением `.ps1` | P0 started |
| `rebuild-worktime-tasks.ps1` | rebuild scheduled tasks | Rust task reconciliation CLI | P1 |
| `fix-session-watchers.ps1` | repair session watchers | Rust repair subcommand | P2 |
| `cleanup-disc-sessions.ps1` | cleanup stale disconnected sessions | Rust maintenance subcommand | P2 |
| `migrate-awatch-rus-paths.ps1` | path migration | Rust migration CLI with backup and dry-run | P1 |
| `deploy-single-user.ps1` | single-user deploy | Rust/Ansible-backed installer action | P1 |
| `deploy-domain-users.ps1` | domain deploy | Rust/Ansible-backed installer action | P1 |
| `deploy-ensemble.ps1` | orchestrated deploy | Rust deploy coordinator or Ansible playbook wrapper | P1 |
| `install-standalone-service.ps1` | local service install | Rust installer/bootstrap CLI | P0 |
| `install-collector-guard-service.ps1` | guard service install | Rust installer/bootstrap CLI | P0 |
| `install-dlp-client.ps1` | DLP client install | Rust installer/bootstrap CLI | P1 |
| `audit-cryptopro.ps1` | CryptoPro audit | Rust audit CLI | P2 |
| `run-user1-probe.ps1` | manual probe | Rust diagnostic probe | P2 |

## 4. Неприкосновенные ограничения

- Не делать big-bang replacement.
- Не отключать PowerShell fallback до прохождения shadow/parity gates.
- Не переносить функции, которые требуют скрытого сбора, keylogging, screen
  recording или content interception. Скриншоты допустимы только для DLP
  incident evidence, если это явно включено политикой.
- Не менять бизнес-логику DLP, 1C, worktime или evidence при переносе.
- Не менять thresholds, bucket names, event schema и ClickHouse schema без
  отдельного решения.
- Не ломать install-kit и rollback ради удаления `.ps1`.
- Не хранить live hostnames, private IP, tokens, passwords или runtime evidence
  paths в публичных tracked docs.

## 5. Общий контракт каждого Rust EXE

Каждый заменяющий EXE должен иметь:

- `--config <path>`;
- `--json` для машинного вывода;
- `--dry-run` для всех действий, меняющих состояние;
- `--timeout-seconds`;
- `--log-path` или structured logging в штатный каталог;
- `--version`;
- стабильные exit codes:
  - `0` - OK;
  - `1` - usage/config/runtime error;
  - `2` - check выполнен, но состояние WARN/FAIL;
  - `3` - action запрещен safety policy;
- structured JSON event для audit trail;
- lock/cooldown для guard/recovery/actions;
- spool/queue для сетевых upload path;
- atomic writes для state files;
- no implicit sudo/admin: повышенные права должны быть видны в task/service
  definition;
- rollback compatibility с текущими config keys.

## 6. Фазы миграции

### Phase 0. Baseline и реестр вызовов

Цель: зафиксировать, где PowerShell реально используется.

Действия:

1. Сканировать репозиторий:
   - `windows/*.ps1`;
   - `windows/installkit/innosetup/*.iss`;
   - `ansible/*.yml`;
   - `ansible/group_vars/*.yml`;
   - `scripts/*`;
   - docs/runbooks.
2. Снять runtime baseline на тестовом Windows host:
   - Scheduled Tasks;
   - Windows Services;
   - текущие command lines процессов;
   - `deployment-config.json`;
   - свежесть AW buckets;
   - наличие DLP evidence и 1C upload.
3. Для каждого скрипта зафиксировать:
   - входные параметры;
   - env/config keys;
   - side effects;
   - event schema;
   - log/state paths;
   - expected exit codes;
   - rollback path.

Выход фазы:

- `docs/POWERSHELL_TO_RUST_ROADMAP_RU.md` как базовая дорожная карта;
- отдельный runtime inventory для live-контура в private/operator notes, без
  публикации секретов.

### Phase 1. Windows Rust foundation

Цель: подготовить общий Windows runtime вместо набора разрозненных EXE.

Действия:

1. Расширить существующие `awatch-agent-rs` и `aw-windows-telemetry` только в
   рамках parity, без новых функций.
2. Вынести общие Windows helpers:
   - config loading;
   - ActivityWatch HTTP client;
   - evidence upload client;
   - Windows task/service inspection;
   - Event Log/EVTX access;
   - filesystem state/spool;
   - structured logs/audit.
3. Зафиксировать единый Windows config contract:
   - `C:\ProgramData\AWatch-rus\deployment-config.json`;
   - `C:\Program Files\AWatch-rus\windows\*.exe`;
   - `C:\ProgramData\AWatch-rus\logs`;
   - `C:\ProgramData\AWatch-rus\spool`;
   - `C:\ProgramData\AWatch-rus\switch-backups`.

Gate:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --release -p awatch-agent-rs -p aw-windows-telemetry
```

### Phase 2. Закрепить уже выполненные замены

Цель: не переписать повторно то, что уже переведено, а довести до production
definition of done.

Компоненты:

- `worktime-session-collector.ps1` -> `awatch-agent-rs`;
- `export-upload-file-1c-telemetry.ps1` -> `aw-windows-telemetry.exe file1c-upload`;
- `sync-dlp-evidence-artifacts.ps1` -> `aw-windows-telemetry.exe dlp-evidence-sync`;
- `aw-collector-guard.ps1` -> `aw-windows-telemetry.exe collector-guard`
  через `AWatchRusCollectorGuard` service wrapper;
- серверный writer -> `aw-1c-ingest-rust`.

Действия:

1. Проверить, что task/service actions запускают EXE, а не PowerShell.
2. Проверить, что PowerShell fallback выключен там, где Rust уже стабилен.
3. Проверить свежесть buckets/ClickHouse/evidence after restart.
4. Обновить install-kit file list: Rust EXE являются primary artifact.
5. Оставить legacy `.ps1` только как rollback на ограниченный период.

Gate:

- `ActivityWatch File1C Upload` запускает `aw-windows-telemetry.exe`;
- `ActivityWatch DLP Evidence Sync` запускает `aw-windows-telemetry.exe`;
- worktime events имеют `source=awatch-agent-rs`;
- `AWatchRusCollectorGuard` запущен и его child process -
  `aw-windows-telemetry.exe collector-guard`;
- guard не создает duplicate legacy browser/fileops/DLP endpoint collectors
  при stale bucket: при уже активных collectors launch tasks не запускаются
  повторно;
- 1C ClickHouse ingest идет по `aw-1c-ingest-rust`;
- PowerShell worktime process count = 0 в штатном режиме.
- PowerShell `aw-collector-guard.ps1` process count = 0 в штатном режиме.

### Phase 3. Runtime collectors

Цель: заменить регулярный сбор данных на Windows.

Порядок:

1. `browser-domains-native-collector.ps1`
   - current status: live Rust primary through
     `aw-windows-telemetry.exe browser-domains-collector`;
   - ActivityWatch window/category health events are written with
     `source=aw-windows-telemetry-rust`;
   - UIAutomation URL/domain extraction implemented in Rust: browser process
     detection, normalized URL, host/rootDomain, default/custom category rules,
     `aw-watcher-web-*`, `aw-detmir-web-category_*` and web DLP incident
     schema preserve the legacy bucket/event contract;
   - live disconnected RDP sessions may legitimately report
     `browserDetected=false` and `urlDetected=false` until a browser is
     foreground in an interactive user session;
   - screenshots только для DLP incident evidence.
2. `dlp-endpoint-signals-collector.ps1`
   - current status: live Rust primary through
     `aw-windows-telemetry.exe dlp-endpoint-collector`;
   - endpoint collector health is written with
     `source=aw-windows-telemetry-rust`;
   - clipboard metadata/hash/length, USB insert, print job metadata and DLP
     incident event semantics implemented in Rust with legacy fields:
     `requestedAction`, `enforcementMode`, `nativeChannelAction`,
     `enforcementSuppressed`, content pack matches and `enforced`;
   - destructive endpoint enforcement for USB write-block and print cancel is
     intentionally not enabled during pilot hardening: Rust emits equivalent
     audit/incident semantics and suppresses unsafe block actions unless a
     separate enforcement decision is made;
   - incident screenshot policy remains: screenshots only for DLP events.
3. `file-operations-collector.ps1`
   - current status: live Rust primary through
     `aw-windows-telemetry.exe file-operations-collector`;
   - bounded filesystem watcher, operation schema, queue/spool and
     create/rename/delete smoke are verified;
   - per-session queue/state/log files are used, so RDP sessions do not share
     one state file;
   - legacy PowerShell remains installed only as rollback/reference.
4. `email-outbound-collector.ps1`
   - metadata-only parity;
   - Outlook/SMTP mode behavior preserved;
   - no content interception beyond current documented behavior.
5. `dlp-policy-client.ps1`
   - Rust policy fetch/cache;
   - strict validation;
   - safe fallback to last known good policy.

Shadow-mode:

- Rust collector writes `source=<rust-component>` and `mode=shadow`;
- PowerShell remains primary during comparison;
- compare event counts, schema, timestamps, severity, policy hits and evidence
  links for at least several collection cycles.

Gate:

- no duplicate management conclusions in portal;
- event schema compatible with current AW/DLP consumers;
- no stale buckets introduced;
- no uncontrolled screenshot capture;
- Rust collector survives AW API outage by spooling and later flushing.

### Phase 4. Guard, service wrapper и recovery

Цель: заменить PowerShell, который управляет процессами и восстановлением.

Компоненты:

- `aw-standalone-service.ps1`;
- `aw-collector-guard.ps1`;
- `hardening-recovery.ps1`;
- `rebuild-worktime-tasks.ps1`;
- `fix-session-watchers.ps1`;
- `cleanup-disc-sessions.ps1`.

Действия:

1. Сделать Rust Windows service wrapper:
   - supervises configured collectors;
   - records child process state;
   - no hidden PowerShell spawn in normal mode.
2. Сделать Rust guard/recovery:
   - allowlist actions only;
   - lock file;
   - cooldown;
   - dry-run by default for destructive or repair actions;
   - audit entry for every change.
3. Перевести rebuild/fix/cleanup в subcommands одного maintenance EXE.

Gate:

- controlled restart of one collector works;
- stale collector detection matches old guard;
- recovery cannot restart arbitrary process/service;
- rollback restores PowerShell guard/service within one operator action.

### Phase 5. Install/deploy/validate

Цель: install-kit перестает запускать PowerShell как штатный bootstrap.

Компоненты:

- `install-standalone-service.ps1`;
- `install-collector-guard-service.ps1`;
- `install-dlp-client.ps1`;
- `deploy-single-user.ps1`;
- `deploy-domain-users.ps1`;
- `deploy-ensemble.ps1`;
- `validate-deployment.ps1`;
- `migrate-awatch-rus-paths.ps1`;
- InnoSetup `AWatch-rus-InnoSetup.iss`;
- Ansible Windows playbooks/group vars.

Действия:

1. Создать Rust bootstrap/installer EXE:
   - install/update service;
   - install/update scheduled tasks;
   - write config atomically;
   - backup previous config/tasks;
   - emit JSON validation report.
2. Перевести validation в Rust:
   - task actions;
   - service status;
   - bucket freshness;
   - file permissions;
   - rust binary version matrix;
   - no PowerShell runtime task check.
3. Обновить InnoSetup:
   - package `*.exe`;
   - run Rust bootstrap;
   - keep `.ps1` out of normal install payload after stabilization.
4. Обновить Ansible:
   - deploy EXE;
   - configure task/service actions to EXE;
   - remove default `.ps1` paths from bot/env after migration.

Gate:

- clean install on test Windows host;
- upgrade from PowerShell install to Rust install;
- rollback to previous package;
- `validate-deployment` JSON is consumed by CI/operator without parsing human
  text;
- install-kit verification confirms no `powershell.exe` in primary install
  action.

### Phase 6. Ops-only хвост

Цель: убрать одноразовые PowerShell helpers.

Компоненты:

- `audit-cryptopro.ps1`;
- `run-user1-probe.ps1`;
- `scripts/powershell/detmir-powershell-profile.ps1`;
- `.pssa_run.ps1`.

Решение:

- `audit-cryptopro.ps1` -> Rust audit CLI;
- `run-user1-probe.ps1` -> Rust diagnostic probe or remove if obsolete;
- operator PowerShell profile is not production runtime and can be retired
  after Rust/SSH operator commands exist;
- `.pssa_run.ps1` removed when no product PowerShell remains.

Gate:

- no production task/service depends on these helpers;
- docs no longer instruct operator to run PowerShell for routine checks;
- one emergency manual path remains documented, but not packaged as runtime.

### Phase 7. Decommission

Цель: убрать PowerShell from product surface.

Действия:

1. Remove `.ps1` from install-kit file list.
2. Remove `powershell.exe` installer run action.
3. Remove default `.ps1` paths from Ansible/bot env.
4. Update architecture/docs:
   - Windows collectors are Rust EXE;
   - PowerShell no longer prerequisite for runtime;
   - rollback history documented separately.
5. Run tracked-file hygiene scan for public docs.

Gate:

```bash
rg -n "powershell\\.exe|\\.ps1|PowerShell" windows ansible docs scripts README.md
```

Expected result:

- only historical notes, explicit rollback docs, or non-runtime examples remain;
- no install-kit primary action launches PowerShell;
- no AWatch-rus Scheduled Task action launches PowerShell in live validation.

## 7. Rollout по хостам

Порядок раскатки:

1. Local build host:
   - build Windows EXE;
   - artifact check;
   - installer dry-run.
2. Test Windows/RDP host:
   - install Rust EXE side-by-side;
   - shadow-mode collectors;
   - compare with PowerShell.
3. Canary production Windows/RDP host:
   - one host, one business day;
   - monitor bucket freshness, DLP incidents, 1C upload, evidence upload.
4. Remaining Windows hosts:
   - staged batches;
   - no more than one failure domain at a time.
5. AW server / Proxmox / gateway:
   - update Ansible/bot references;
   - verify server Rust services remain active;
   - verify ClickHouse ingest and portal health.
6. Install-kit:
   - publish Rust-first package;
   - keep previous package as rollback artifact.

## 8. Acceptance gates

Local gates:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --release
cargo build --release --target x86_64-pc-windows-gnu -p awatch-agent-rs -p aw-windows-telemetry
```

Repository gates:

```bash
scripts/check_detmir_rust_release_artifacts.sh
scripts/verify_innosetup_installer.sh
git diff --check
```

Windows gates:

```powershell
Get-ScheduledTask | Where-Object {
  $_.TaskName -like 'ActivityWatch*' -or $_.TaskName -like 'AWatch*'
} | Select-Object TaskName,TaskPath,State
```

Expected:

- task actions point to Rust EXE for migrated components;
- no migrated component runs `powershell.exe`;
- logs are fresh;
- spool is empty or draining;
- AW buckets are fresh;
- DLP evidence sync only uploads DLP incident evidence;
- 1C upload does not copy screenshots.

Server gates:

```bash
detmir-status --json
detmir-check --json
systemctl --failed --no-pager
systemctl list-timers aw-1c-ingest.timer --no-pager
```

Expected:

- AWatch-rus severity is OK or explained WARN;
- no failed Rust services;
- ClickHouse writer timer is active;
- portal data freshness is acceptable for pilot.

## 9. Rollback model

До decommission каждая миграция хранит rollback:

- previous EXE/script backup in `switch-backups`;
- previous task/service definition;
- previous `deployment-config.json`;
- one-command switch back for canary host;
- rollback reason written to audit log.

Rollback триггеры:

- AW bucket stale/dead after migration;
- DLP event loss or uncontrolled duplicate events;
- evidence upload fails repeatedly and spool grows;
- 1C upload stops producing landing files;
- service/guard restarts loop;
- Windows host shows sustained CPU/RAM regression from new EXE;
- user-visible pilot portal data quality degrades.

## 10. Риски

| Риск | Где | Снижение риска |
|---|---|---|
| Windows API differs from PowerShell cmdlets | DLP, tasks, Event Log, print, WMI | parity fixtures, canary, schema comparison |
| UIAutomation/browser URL extraction changes behavior | browser collector | shadow-mode and domain count comparison |
| Outlook/SMTP metadata behavior differs | email collector | metadata-only parity, explicit mode tests |
| Privilege mismatch | services/tasks/recovery | install under same account, explicit elevation, validation |
| AV/EDR blocks unsigned EXE | Windows hosts | code signing plan, allowlist, staged rollout |
| Duplicate events during shadow | all collectors | shadow source tags, portal ignores shadow for KPI |
| Rollback not fast enough | production canary | backup tasks/configs, single switch command |
| Public docs leak live contour details | tracked docs | placeholders and `git grep` hygiene scan |

## 11. Минимальный порядок ближайших работ

1. Зафиксировать live inventory PowerShell usage на тестовом Windows/RDP host.
2. Закрепить уже сделанные Rust paths:
   - worktime/RDP;
   - 1C file upload;
   - DLP evidence sync.
3. Расширить `aw-windows-telemetry.exe validate-deployment` до полной parity с
   `validate-deployment.ps1`, потому что он станет главным gate для следующих
   замен.
4. Перенести `aw-standalone-service.ps1` и `aw-collector-guard.ps1`, потому
   что они управляют runtime collectors.
5. Стабилизировать P0 runtime collectors после live Rust-primary switch:
   - browser domains;
   - DLP endpoint signals;
   - file operations.
   URL/domain extraction и clipboard/USB/print incident semantics закрыты на
   уровне Rust code/schema/runtime self-test. Следующий шаг перед удалением
   legacy `.ps1` - burn-in, canary rollback test и live foreground-browser
   proof в интерактивной RDP-сессии.
6. Перенести deploy/install scripts и InnoSetup primary action.
7. Убрать PowerShell paths из Ansible/bot env.
8. Удалить `.ps1` из install-kit и оставить только historical rollback docs.

## 12. Итоговая оценка

Миграция реалистична, потому что серверная часть уже Rust-first, а Windows
контур уже имеет два рабочих Rust основания: `awatch-agent-rs` и
`aw-windows-telemetry.exe`.

Критичный участок не сервер, а Windows runtime и install-kit:

- `dlp-endpoint-signals-collector.ps1`;
- `browser-domains-native-collector.ps1`;
- `aw-collector-guard.ps1`;
- `aw-standalone-service.ps1`;
- `validate-deployment.ps1`;
- InnoSetup action that still launches PowerShell.

До пилота не нужно удалять весь PowerShell хвост. Для пилота достаточно
закрепить уже работающие Rust-primary paths, не допустить копирования
скриншотов в 1C контуре, оставить скриншоты только для DLP events, иметь
понятный rollback и честно формулировать границу parity: code/schema/runtime
path закрыт, а полный live URL/domain incident proof требует активного
foreground browser в интерактивной RDP-сессии.
Полное удаление PowerShell из install-kit лучше делать после canary и shadow
parity по DLP/browser/file collectors.
