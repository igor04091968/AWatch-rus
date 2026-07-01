# AWatch-rus: план развития на следующие 6-12 месяцев

Дата аудита: 2026-07-01.

Статус: development planning после анализа текущего репозитория. Документ не
добавляет новые продуктовые функции и не меняет runtime-поведение. Цель -
снизить эксплуатационные, release, security и maintainability риски без
нарушения обратной совместимости.

## Границы плана

- Production уже существует; текущий DetMir контур рассчитан примерно на 5 RDP
  пользователей.
- Рабочие подсистемы не переписываются. Все изменения должны быть
  инкрементальными, проверяемыми и откатываемыми.
- DLP runtime остается `core_only/disabled` по умолчанию; `light` включается
  только вручную после resource preflight.
- Loki и always-on Velociraptor не включаются как часть этого плана.
- PowerShell fallback на Windows сохраняется до доказанной parity и burn-in
  Rust-пути.
- GitHub остается public mirror validation; release evidence для российского
  контура должен формироваться отдельно.

## Фактическое состояние репозитория

- Основной runtime слой стал Rust-first: `adk-rust/` содержит 58 workspace
  crates; локальный `cargo metadata --locked` разрешает 349 пакетов.
- Rust dependency hygiene уже включен: `cargo audit --deny warnings`,
  `cargo deny`, `cargo machete --with-metadata`, `cargo tree --duplicates` есть
  в workflow или локальных gate. Текущий аудит: `cargo audit` и `cargo machete`
  чистые, `cargo deny` проходит, но оставляет 36 `bans` warnings.
- CI уже покрывает Rust, docs/registry, smoke, operational maturity,
  dependency hygiene, security scan и release binary build. Есть drift:
  несколько workflows используют floating `stable`, хотя `rust-toolchain.toml`
  закрепляет `1.94.0`.
- DetMir DLP resource guardrails реализованы: runtime disabled/core-only by
  default, `detmir-dlp-load-guard`, documented rollback, skipped DLP buckets в
  smoke считаются нормой.
- Security Finding Inbox, Hayabusa и Velociraptor оформлены как optional
  findings/forensics sources и не являются hot-path зависимостью Workforce.
- Документация registry/governance сильная, но `docs/PROJECT_STATUS_RU.md` и
  `docs/RESIDUAL_RISKS_RU.md` фиксируют открытые gaps: Gitea restore drill,
  Russian build-runner, первый release evidence build, legal/rightsholder
  package, external review evidence.
- Самые крупные maintainability hotspots:
  `adk-rust/crates/detmir-portal/src/main.rs` - 14200 строк,
  `adk-rust/crates/aw-windows-telemetry/src/main.rs` - 6411 строк,
  `proxmox/tsj_guardian_bot.py` - 4610 строк,
  `adk-rust/crates/worktime-api/src/main.rs` - 3988 строк,
  `ansible/deploy_aw_server.yml` - 3099 строк.
- Найдены точные дубликаты скриптов:
  `scripts/aw-contour-diag.sh` =
  `scripts/detmir-full-diagnostics/aw-contour-diag.sh`;
  `scripts/check_production_inventory_placeholders.sh` =
  `scripts/detmir-full-diagnostics/check_production_inventory_placeholders.sh`.
- Найден security hygiene gap: ClickHouse/1C ops wrappers передают
  `CLICKHOUSE_PASSWORD` через `--password`, что раскрывает секрет в process
  argv на хосте.

## P0 - критично для production

### P0-1. Gate фактической свежести production Rust-бинарников

- Почему нужно: `scripts/check_detmir_rust_release_artifacts.sh` проверяет
  наличие локальных release artifacts, но не доказывает, что production
  service/timer/Windows task реально запускает тот же binary SHA256.
- Риск: stale бинарник в production может отличаться от проверенного кода,
  усложнить rollback и скрыть регрессию в high-load контуре.
- Ожидаемый эффект: воспроизводимая цепочка `service/timer/task -> binary path
  -> crate -> local artifact -> prod sha256 -> git sha`.
- Сложность: средняя.
- Объем работ: 3-5 инженерных дней.
- Затрагиваемые файлы:
  `scripts/check_detmir_rust_release_artifacts.sh`,
  `scripts/package_rust_release_binaries.py`,
  `scripts/detmir-full-diagnostics/`,
  `adk-rust/crates/detmir-readiness/`,
  `windows/validate-deployment.ps1`,
  `docs/DETMIR_CURRENT_STATE_RU.md`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.
- Критерии приемки:
  команда dry-run выводит все реально задействованные binaries на
  `10.10.10.2`, `10.10.10.13` и Windows RDP host;
  для каждого binary есть crate, role, unit/timer/task, prod SHA256 и local
  release SHA256;
  gate падает при missing/stale binary;
  проверка не включает heavy DLP, Loki или always-on Velociraptor;
  rollback path документирован и проверен на одном canary binary.

### P0-2. Bounded retention для state, evidence, queues и diagnostic output

- Почему нужно: DLP/evidence/backlog/diagnostic контуры уже имеют guardrails,
  но retention/cleanup policy для исторических artifacts остается отдельной
  future work; disk exhaustion на Proxmox/AW server является прямым outage
  риском.
- Риск: накопление JSONL queues, screenshots, Hayabusa drops, diagnostic runs,
  ClickHouse landing files или локальных state snapshots может забить диск и
  остановить ingestion, portal или ActivityWatch.
- Ожидаемый эффект: предсказуемое потребление диска, безопасный dry-run cleanup,
  понятный rollback через backup/hold rules.
- Сложность: средняя.
- Объем работ: 4-7 инженерных дней.
- Затрагиваемые файлы:
  `adk-rust/crates/aw-prune-local-state/`,
  `scripts/detmir-full-diagnostics/`,
  `scripts/detmir_dlp_warehouse_sync.sh`,
  `aw-server/logrotate.conf`,
  `windows/validate-deployment.ps1`,
  `adk-rust/crates/aw-windows-telemetry/`,
  `docs/DLP_OPTIONAL_RUNTIME_RU.md`,
  `docs/DLP_RESOURCE_PROFILES_RU.md`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.
- Критерии приемки:
  есть единая retention matrix по путям, владельцам, age/size limits и dry-run;
  cleanup never follows untrusted paths and only touches allowlisted roots;
  Windows queues имеют max-age/max-size validation;
  skipped DLP buckets при `AW_DLP_ENABLED=false` не считаются ошибкой;
  smoke показывает, что cleanup не удаляет active state и не требует downtime.

### P0-3. Убрать секреты ClickHouse/1C из process argv

- Почему нужно: `clickhouse-1c/ops/run_*.sh` передают пароль как
  `--password "${CLICKHOUSE_PASSWORD}"`; этот аргумент виден через process list
  локальному пользователю или диагностике.
- Риск: утечка ClickHouse credentials с возможностью чтения/изменения
  production analytics data.
- Ожидаемый эффект: секреты передаются через env/config file descriptor или
  другой non-argv канал; logs и error output остаются redacted.
- Сложность: низкая-средняя.
- Объем работ: 2-4 инженерных дня.
- Затрагиваемые файлы:
  `clickhouse-1c/ops/run_ingest_cycle.sh`,
  `clickhouse-1c/ops/run_manager_brief.sh`,
  `clickhouse-1c/ops/run_recovery_brief.sh`,
  `clickhouse-1c/ops/run_company_registry_bindings_refresh.sh`,
  `clickhouse-1c/ops/run_company_intelligence_refresh.sh`,
  `clickhouse-1c/ops/check_ingest_freshness.sh`,
  `clickhouse-1c/ai/*.py`,
  `clickhouse-1c/etl/*.py`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.
- Критерии приемки:
  `rg -- '--password \"\\$\\{CLICKHOUSE_PASSWORD\\}\"' clickhouse-1c` ничего не
  находит в runtime wrappers;
  smoke через `ps` доказывает отсутствие password в argv;
  существующие env-based deployments остаются совместимыми;
  failed auth logs не печатают password;
  `bash -n clickhouse-1c/ops/*.sh` и affected Python smoke проходят.

## P1 - желательно выполнить до следующего релиза

### P1-1. Выровнять Rust toolchain во всех CI workflows

- Почему нужно: `rust-toolchain.toml` закрепляет `1.94.0`, но
  `.github/workflows/ci.yml`, `security.yml`, `coverage.yml`,
  `dependency-hygiene.yml` и `release-assets.yml` используют floating
  `dtolnay/rust-toolchain@stable`.
- Риск: CI и release runner могут собирать разные версии компилятора, что
  снижает воспроизводимость и усложняет анализ regressions.
- Ожидаемый эффект: единый compiler baseline для local, GitHub mirror и
  будущего российского build-runner.
- Сложность: низкая.
- Объем работ: 1-2 инженерных дня.
- Затрагиваемые файлы:
  `rust-toolchain.toml`,
  `.github/workflows/ci.yml`,
  `.github/workflows/security.yml`,
  `.github/workflows/coverage.yml`,
  `.github/workflows/dependency-hygiene.yml`,
  `.github/workflows/release-assets.yml`,
  `.github/workflows/rust-binary-build.yml`,
  `.github/workflows/rust-workspace.yml`,
  `docs/QUALITY_STATUS_RU.md`.
- Критерии приемки:
  все blocking Rust jobs используют `rust-toolchain.toml` или тот же explicit
  channel;
  nightly остается только для advisory `cargo udeps`;
  CI check names не меняются;
  локально и в CI проходят `cargo fmt`, `cargo test`, `cargo clippy -D warnings`.

### P1-2. Staged triage для duplicate/deprecated Rust dependencies

- Почему нужно: `cargo deny` проходит, но `cargo tree --duplicates` показывает
  дубли `bitflags`, `getrandom`, `hashbrown`, `mio`, `zip`, `windows-*`;
  `serde_yaml 0.9.34+deprecated` уже зафиксирован как средний риск.
- Риск: supply-chain surface и binary size растут, а устаревшие crates могут
  стать будущим vulnerability blocker.
- Ожидаемый эффект: baseline по допустимым дублям, запрет новых необоснованных
  дублей, replacement path для deprecated crates.
- Сложность: средняя.
- Объем работ: 4-8 инженерных дней.
- Затрагиваемые файлы:
  `adk-rust/Cargo.toml`,
  `adk-rust/Cargo.lock`,
  `deny.toml`,
  `.github/workflows/dependency-hygiene.yml`,
  crates using `serde_yaml`, `calamine`, `notify`, `zip`, `windows-sys`,
  `docs/THIRD_PARTY_LICENSES_RU.md`,
  `docs/QUALITY_STATUS_RU.md`.
- Критерии приемки:
  для каждого duplicate family есть decision: keep with reason, update, or
  remove;
  `cargo audit --deny warnings` и `cargo deny ...` проходят;
  новые duplicates без allowlist/обоснования блокируются;
  replacement plan для `serde_yaml` не ломает существующие YAML configs;
  изменения dependency graph идут отдельными PR, не смешиваются с runtime
  features.

### P1-3. Load regression harness для portal/worktime/report hot paths

- Почему нужно: `docs/PROJECT_STATUS_RU.md` прямо фиксирует residual risk:
  full report/snapshot prewarm может быть CPU/IO дорогим. Сейчас production
  малый, но расширение users/events без бюджета нагрузки рискованно.
- Риск: рост RDP users или history window приводит к slow portal, stale cache,
  ClickHouse/AW amplification или datastore lock pressure.
- Ожидаемый эффект: измеримые budgets для p95 latency, memory, query count,
  cache hit/stale behavior и degradation semantics.
- Сложность: средняя-высокая.
- Объем работ: 1-2 недели.
- Затрагиваемые файлы:
  `scripts/operational-maturity-check.mjs`,
  `scripts/awatch-production-hardening-smoke.mjs`,
  `adk-rust/crates/detmir-portal/`,
  `adk-rust/crates/worktime-api/`,
  `adk-rust/crates/worktime-prewarm/`,
  `adk-rust/crates/aw-contour-smoke/`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`,
  `docs/QUALITY_STATUS_RU.md`.
- Критерии приемки:
  synthetic dataset проверяет минимум 5/20/50 users without prod data;
  harness фиксирует p95 latency, max RSS, query count and stale-cache decisions;
  disconnected RDP sessions and event-driven buckets не дают false-fail;
  `AW_DLP_ENABLED=false` profile respected;
  heavy job остается scheduled/advisory, blocking smoke остается быстрым.

### P1-4. Windows Rust validation parity before reducing PowerShell runtime

- Почему нужно: `docs/POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md` показывает, что
  Rust overlay уже основной для части collectors, но `validate-deployment.ps1`,
  Hayabusa upload и некоторые recovery paths остаются fallback/runtime.
- Риск: преждевременное удаление PowerShell fallback ухудшит recovery на
  Windows Server 2019 и localized RDP sessions.
- Ожидаемый эффект: доказанная parity Rust validation, safe fallback policy и
  меньше runtime drift.
- Сложность: средняя.
- Объем работ: 1-2 недели.
- Затрагиваемые файлы:
  `adk-rust/crates/aw-windows-telemetry/`,
  `windows/validate-deployment.ps1`,
  `windows/export-upload-hayabusa-to-aw-server.ps1`,
  `windows/ActivityWatch.Windows.Common.psm1`,
  `ansible/deploy_aw_windows.yml`,
  `docs/POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md`,
  `docs/POWERSHELL_TO_RUST_ROADMAP_RU.md`.
- Критерии приемки:
  Rust `validate-deployment` покрывает все 7 секций PowerShell validation;
  CP866/localized user handling covered by tests;
  canary run сравнивает Rust и PowerShell reports на одном RDP host;
  PowerShell fallback остается documented rollback;
  no removal before burn-in evidence.

### P1-5. Российский build-runner и первый release evidence build

- Почему нужно: `ROADMAP.md`, `docs/PROJECT_STATUS_RU.md` и
  `docs/RESIDUAL_RISKS_RU.md` фиксируют, что `awatch-build-01` planned, а
  первый release evidence build pending.
- Риск: registry/release package cannot be claimed reproducible in target
  Russian contour.
- Ожидаемый эффект: проверяемый release package: source archive, binaries,
  SBOM, SHA256SUMS, cargo metadata/tree, logs, manifest.
- Сложность: средняя.
- Объем работ: 3-6 инженерных дней плюс инфраструктурное окно.
- Затрагиваемые файлы:
  `scripts/build_release_evidence.sh`,
  `scripts/check_release_evidence.sh`,
  `scripts/verify_release_assets.sh`,
  `docs/registry/RU_BUILD_RUNNER_READINESS_RU.md`,
  `docs/registry/BUILD_RUNNER_SETUP_RUNBOOK_RU.md`,
  `docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md`,
  `docs/registry/registry-evidence-manifest.json`.
- Критерии приемки:
  build-runner имеет documented OS/toolchain;
  release evidence build выполнен не на GitHub runner;
  `scripts/check_release_evidence.sh` проходит;
  artifacts имеют SHA256 и manifest;
  public docs не заявляют registry completion раньше legal approval.

### P1-6. Gitea backup restore drill

- Почему нужно: backup и SHA256 documented, но `restore_tested=false` остается
  открытым residual risk.
- Риск: disaster recovery process может оказаться неполным только во время
  инцидента.
- Ожидаемый эффект: доказанный restore на отдельном хосте и понятный RTO/RPO
  baseline.
- Сложность: средняя.
- Объем работ: 2-4 инженерных дня.
- Затрагиваемые файлы:
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`,
  `docs/registry/registry-evidence-manifest.json`,
  `scripts/registry_readiness_check.sh`,
  `docs/PROJECT_STATUS_RU.md`,
  `docs/RESIDUAL_RISKS_RU.md`.
- Критерии приемки:
  restore выполнен на отдельном сервере;
  checksums, logs, repository accessibility и rollback notes сохранены;
  manifest обновлен с `restore_tested=true`;
  recovery evidence не содержит секреты.

### P1-7. Hayabusa upload path: Rust helper parity and operational proof

- Почему нужно: матрица PowerShell-to-Rust помечает
  `windows/export-upload-hayabusa-to-aw-server.ps1` как P1 runtime wrapper.
  Hayabusa is optional, but it is a security evidence path.
- Риск: Windows PowerShell encoding, task principal или SSH wrapper drift
  ломает forensics upload без явной compile-time проверки.
- Ожидаемый эффект: Rust helper covers upload, sidecar metadata, checksum and
  server drop-zone contract; PowerShell remains fallback.
- Сложность: средняя.
- Объем работ: 5-8 инженерных дней.
- Затрагиваемые файлы:
  `adk-rust/crates/aw-windows-telemetry/`,
  `adk-rust/crates/hayabusa-tools/`,
  `windows/export-upload-hayabusa-to-aw-server.ps1`,
  `aw-server/hayabusa/aw-hayabusa.sh`,
  `ansible/deploy_aw_windows.yml`,
  `docs/POWERSHELL_SCRIPT_STATUS_MATRIX_RU.md`,
  `docs/POWERSHELL_TO_RUST_ROADMAP_RU.md`.
- Критерии приемки:
  helper creates identical server-side intake result on fixture;
  BOM/backslash/path traversal tests preserved;
  task principal remains interactive Administrator where required;
  server `aw-hayabusa-drop.path` processing stays optional and fail-closed;
  no automatic remediation is introduced.

## P2 - улучшения архитектуры и сопровождаемости

### P2-1. Инкрементальная декомпозиция крупных runtime modules

- Почему нужно: несколько production-critical files слишком крупные для
  надежного review и локального reasoning, особенно `detmir-portal/main.rs` и
  `aw-windows-telemetry/main.rs`.
- Риск: высокий change-coupling, труднее делать точечные fixes и находить
  regressions.
- Ожидаемый эффект: smaller modules by domain, faster review, focused tests.
- Сложность: средняя-высокая.
- Объем работ: 2-4 недели несколькими малыми PR.
- Затрагиваемые файлы:
  `adk-rust/crates/detmir-portal/src/main.rs`,
  `adk-rust/crates/detmir-portal/src/*`,
  `adk-rust/crates/aw-windows-telemetry/src/main.rs`,
  `adk-rust/crates/worktime-api/src/main.rs`,
  `proxmox/tsj_guardian_bot.py`,
  `clickhouse-1c/ai/company_intelligence_api.py`.
- Критерии приемки:
  каждый PR только moves/extracts one bounded domain;
  public API, config names, systemd units and task names unchanged;
  tests before/after identical;
  no new dependencies without written justification;
  line count decreases in target `main.rs` files without behavioral rewrite.

### P2-2. Consolidate exact duplicate diagnostic scripts

- Почему нужно: две пары скриптов имеют identical SHA256, что создает риск
  будущего drift при исправлениях.
- Риск: one copy fixed, bundled copy stale; diagnostics disagree.
- Ожидаемый эффект: single source of truth while preserving existing paths.
- Сложность: низкая.
- Объем работ: 1-2 инженерных дня.
- Затрагиваемые файлы:
  `scripts/aw-contour-diag.sh`,
  `scripts/detmir-full-diagnostics/aw-contour-diag.sh`,
  `scripts/check_production_inventory_placeholders.sh`,
  `scripts/detmir-full-diagnostics/check_production_inventory_placeholders.sh`,
  `scripts/detmir-full-diagnostics/detmir-full-diagnostics.sh`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.
- Критерии приемки:
  оба старых paths продолжают работать;
  есть один canonical implementation или generated copy check;
  CI/quality gate ловит future drift;
  shellcheck/bash syntax pass.

### P2-3. Split oversized Ansible deployment playbook without behavior change

- Почему нужно: `ansible/deploy_aw_server.yml` имеет 3099 строк; это усложняет
  review и безопасные partial changes.
- Риск: accidental deploy behavior changes in unrelated task blocks.
- Ожидаемый эффект: role/include structure with same task order and clearer
  ownership.
- Сложность: средняя.
- Объем работ: 1-2 недели.
- Затрагиваемые файлы:
  `ansible/deploy_aw_server.yml`,
  `ansible/roles/`,
  `ansible/group_vars/`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.
- Критерии приемки:
  `ansible-playbook --syntax-check` passes;
  `--list-tasks` before/after is reviewed for ordering parity;
  no default vars changed;
  rollback is reverting includes to previous single playbook.

### P2-4. Bound 1C ingest memory and file processing profile

- Почему нужно: `adk-rust/crates/aw-1c-ingest/src/main.rs` currently reads rows
  into `Vec` and builds large JSON batches; acceptable for pilot, but risky for
  larger exports.
- Риск: memory spikes, long transaction windows, ClickHouse insert amplification
  on larger 1C files.
- Ожидаемый эффект: chunked processing where practical, explicit limits and
  predictable failure mode for oversized files.
- Сложность: средняя-высокая.
- Объем работ: 1-2 недели.
- Затрагиваемые файлы:
  `adk-rust/crates/aw-1c-ingest/src/main.rs`,
  `clickhouse-1c/etl/config.yml`,
  `clickhouse-1c/etl/config.example.yml`,
  `clickhouse-1c/sql/`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`,
  `docs/QUALITY_STATUS_RU.md`.
- Критерии приемки:
  synthetic large CSV/XLSX fixture documents max RSS and runtime;
  oversized input fails closed with clear diagnostic;
  existing small DetMir files produce identical rows;
  ClickHouse insert batch size is configurable and bounded.

### P2-5. Systematic command execution boundary audit

- Почему нужно: `detmir-portal` already validates shell probe commands and
  kills process groups on timeout, but other operational binaries also execute
  shell/commands (`aw-slo-monitor`, `diag-and-manual-restart`, quality tooling).
- Риск: future config drift can reintroduce command injection, secrets in argv
  or incomplete timeout cleanup.
- Ожидаемый эффект: documented command execution policy and shared tests for
  allowlisted commands, timeouts, redaction and process-group cleanup.
- Сложность: средняя.
- Объем работ: 4-7 инженерных дней.
- Затрагиваемые файлы:
  `adk-rust/crates/detmir-portal/src/main.rs`,
  `adk-rust/crates/detmir-portal/src/production/limits.rs`,
  `adk-rust/crates/aw-slo-monitor/src/main.rs`,
  `adk-rust/crates/diag-and-manual-restart/src/main.rs`,
  `adk-rust/crates/quality-gate/src/main.rs`,
  `docs/SECURITY_OPERATIONS_RUNBOOK_RU.md`.
- Критерии приемки:
  every runtime command source is classified as constant, allowlisted config or
  operator-only diagnostic;
  tests reject shell control operators where config-driven;
  timeout tests verify child/grandchild cleanup;
  logs never include secrets from env or args.

### P2-6. Install kit artifact reproducibility and stale payload checks

- Почему нужно: install kit is critical for Windows deployment, includes large
  LFS artifacts, and must not silently ship stale payloads.
- Риск: Windows production receives an older collector or mismatched config
  while repo checks pass.
- Ожидаемый эффект: deterministic installer evidence and stronger
  install-kit-vs-repo validation.
- Сложность: средняя.
- Объем работ: 5-8 инженерных дней.
- Затрагиваемые файлы:
  `windows/installkit/innosetup/`,
  `adk-rust/crates/check-install-kit-vs-repo/`,
  `adk-rust/crates/rebuild-install-kit/`,
  `adk-rust/crates/validate-install-kit/`,
  `adk-rust/crates/verify-innosetup-installer/`,
  `scripts/rebuild_install_kit.sh`,
  `docs/INSTALL_KIT_RUNBOOK_RU.md`.
- Критерии приемки:
  installer manifest lists exact source commit and payload SHA256;
  validation fails on stale collector payload;
  generated artifacts are not confused with source files;
  Windows task names and config schema remain backward compatible.

### P2-7. Standardize operational metrics contract across Rust daemons

- Почему нужно: individual services expose useful fields, but there is no
  consistent minimal metrics/diagnostics contract for every long-running Rust
  daemon.
- Риск: incident response depends on service-specific knowledge; regressions in
  one daemon are harder to compare with another.
- Ожидаемый эффект: consistent `/health`, structured log fields and optional
  metrics snapshots for status, cache, queue, retry, timeout and resource
  pressure.
- Сложность: средняя.
- Объем работ: 2-3 недели staged by service.
- Затрагиваемые файлы:
  `adk-rust/crates/aw-rus-healthd/`,
  `adk-rust/crates/worktime-api/`,
  `adk-rust/crates/detmir-portal/`,
  `adk-rust/crates/dlp-*`,
  `adk-rust/crates/aw-1c-ingest/`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`,
  `grafana/`.
- Критерии приемки:
  minimal contract documented;
  each daemon reports version/build, config profile, degraded reason and queue
  pressure where applicable;
  existing endpoints remain backward compatible;
  Grafana/dashboard changes are additive.

## P3 - долгосрочные улучшения

### P3-1. Coverage thresholds after baseline review

- Почему нужно: coverage workflow exists, but roadmap explicitly says threshold
  should be added after baseline review.
- Риск: coverage can silently decline while CI remains green.
- Ожидаемый эффект: gradual coverage floor without slowing blocking jobs.
- Сложность: средняя.
- Объем работ: 1-2 недели.
- Затрагиваемые файлы:
  `.github/workflows/coverage.yml`,
  `adk-rust/`,
  `docs/QUALITY_STATUS_RU.md`,
  `docs/REVIEW_CHECKLIST_RU.md`.
- Критерии приемки:
  initial threshold is based on measured baseline, not arbitrary target;
  critical crates get stricter per-crate goals;
  generated/fixture code excluded consistently;
  threshold moves from advisory to blocking only after stable history.

### P3-2. Russian OS compatibility matrix

- Почему нужно: roadmap keeps Russian OS compatibility as planned validation.
- Риск: deployment assumptions can fail on target OS variants only after a
  customer deployment attempt.
- Ожидаемый эффект: explicit supported/unsupported OS matrix and installation
  evidence.
- Сложность: средняя-высокая.
- Объем работ: 2-4 недели depending on available test hosts.
- Затрагиваемые файлы:
  `docs/registry/`,
  `docs/INSTALLATION.md`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`,
  `ansible/`,
  `windows/`,
  `windows/installkit/`.
- Критерии приемки:
  matrix lists OS version, role, test date, result and limitations;
  unsupported combinations are documented explicitly;
  no production defaults are changed only for compatibility claims.

### P3-3. Reduce single-maintainer operational risk

- Почему нужно: `docs/RESIDUAL_RISKS_RU.md` identifies one-main-developer risk
  and pending reviewed PR evidence.
- Риск: incident response and release continuity depend too much on one person.
- Ожидаемый эффект: documented ownership, review evidence and handoff paths.
- Сложность: организационная, средняя.
- Объем работ: 1-3 месяца part-time.
- Затрагиваемые файлы:
  `.github/CODEOWNERS`,
  `.github/pull_request_template.md`,
  `docs/PR_REVIEW_WORKFLOW_RU.md`,
  `docs/PR_REVIEW_EVIDENCE_RU.md`,
  `docs/REVIEW_CHECKLIST_RU.md`,
  `docs/RESIDUAL_RISKS_RU.md`.
- Критерии приемки:
  at least one reviewed PR merged without bypass;
  release branch review policy documented;
  emergency maintainer handoff checklist exists;
  residual risk status updated only with real evidence.

### P3-4. Historical docs and naming hygiene

- Почему нужно: older docs still reference staged statuses, older check names
  or public-readiness context; this increases cognitive load for maintainers.
- Риск: operators follow outdated docs or confuse GitHub public mirror checks
  with Russian release evidence.
- Ожидаемый эффект: clearer current-state docs and archived historical records.
- Сложность: низкая-средняя.
- Объем работ: 3-6 инженерных дней.
- Затрагиваемые файлы:
  `docs/PROJECT_STATUS_RU.md`,
  `docs/ROADMAP_CONFORMANCE_AUDIT_RU.md`,
  `docs/QUALITY_STATUS_RU.md`,
  `docs/registry/`,
  `README.md`,
  `ROADMAP.md`.
- Критерии приемки:
  current-state docs contain current required check names;
  historical docs are marked historical and not operational runbooks;
  no product claims are strengthened without evidence;
  broken/stale links are corrected.

### P3-5. Long-horizon capacity sizing guide

- Почему нужно: current DetMir production is small; future deployments need
  sizing guidance for AW SQLite, ClickHouse, Influx/Grafana, DLP light profile
  and Windows collector load.
- Риск: deployments scale by guesswork and overload the small-Proxmox pattern.
- Ожидаемый эффект: sizing table by users/events/day, retention, disk, CPU/RAM
  and optional security contours.
- Сложность: средняя.
- Объем работ: 2-4 недели after P1 load harness data exists.
- Затрагиваемые файлы:
  `docs/DLP_RESOURCE_PROFILES_RU.md`,
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`,
  `docs/DETMIR_CURRENT_STATE_RU.md`,
  `grafana/`,
  `scripts/operational-maturity-check.mjs`.
- Критерии приемки:
  guide is based on measured data, not estimates only;
  separate profiles exist for 5, 20, 50 and 100 users;
  optional DLP/Hayabusa/Velociraptor resource costs are explicit;
  default production profile remains conservative.

## Не включено, потому что уже реализовано

- Включение dependency hygiene automation: `cargo machete`, `cargo deny`,
  `cargo audit`, `cargo tree --duplicates`, advisory `cargo udeps` уже есть в
  workflow.
- Удаление текущих unused Rust dependencies: текущий `cargo machete
  --with-metadata` не нашел unused dependencies.
- DetMir DLP light/core-only guardrails: runtime control, resource profiles,
  load guard and skipped-bucket semantics already implemented.
- Security Finding Inbox and optional Hayabusa/Velociraptor findings path:
  schema, portal/API and fail-closed executor path already documented as
  optional and approval-driven.
- Operational maturity smoke workflow: already exists and explicitly avoids
  heavy DLP/Loki/always-on Velociraptor.
- Branch protection/CODEOWNERS/PR checklist basics: already documented; the
  remaining work is evidence and regular use, not creating the mechanism.

## Проверки, использованные для аудита

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo metadata --locked --format-version 1
cargo tree --duplicates --locked
cargo audit --deny warnings
cargo deny --manifest-path Cargo.toml check --config ../deny.toml --hide-inclusion-graph --show-stats
cargo machete --with-metadata

cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
rg -n "dtolnay/rust-toolchain@(stable|[0-9])|rust-toolchain" .github/workflows rust-toolchain.toml
rg -n "CLICKHOUSE_PASSWORD|--password" clickhouse-1c/ops clickhouse-1c/ai clickhouse-1c/etl
sha256sum scripts/aw-contour-diag.sh scripts/detmir-full-diagnostics/aw-contour-diag.sh
sha256sum scripts/check_production_inventory_placeholders.sh scripts/detmir-full-diagnostics/check_production_inventory_placeholders.sh
wc -l adk-rust/crates/detmir-portal/src/main.rs adk-rust/crates/aw-windows-telemetry/src/main.rs proxmox/tsj_guardian_bot.py adk-rust/crates/worktime-api/src/main.rs ansible/deploy_aw_server.yml
```
