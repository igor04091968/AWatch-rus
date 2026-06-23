# AWatch-rus: PR / code review checklist

Дата: 2026-06-22

Статус: advisory checklist for public review process.

Этот документ описывает проверочный чеклист для pull requests и внешнего
инженерного review. Он не утверждает, что внешний peer review уже выполняется
регулярно, и не является гарантией отсутствия дефектов или уязвимостей.

GitHub Actions используется только как public mirror validation. Registry
release evidence должен производиться на российском build-runner.

## Общая безопасность изменений

- Изменение имеет понятную цель, ограниченный scope and documented impact.
- Нет секретов, токенов, паролей, приватных ключей, recovery codes or live
  credentials.
- Нет персональных данных сотрудников, реальных employee logs or customer
  evidence.
- Нет реальных IP, hostname or infrastructure identifiers заказчика.
- Нет новых публичных портов, external callbacks or telemetry without explicit
  documentation.
- Нет ослабления fail-closed checks, security gates or auditability.

## Rust code quality

- Rust-код форматируется `cargo fmt --all --check`.
- Для существенных Rust-изменений ожидается полный workspace gate:
  `cargo test --workspace --all-targets --locked`,
  `cargo test --workspace --doc --locked` и
  `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Для Windows/RDP collector дополнительно проверяется target
  `x86_64-pc-windows-gnu` через `cargo check` и `cargo clippy`.
- Ошибки обрабатываются явно; нет silent fallback для security-sensitive paths.
- Timeouts, retries and bounds are explicit for network or long-running work.
- Новые dependencies justified and license-compatible.

## API / contract compatibility

- Public API, CLI flags, file formats and JSON contracts remain compatible, or
  breaking impact is explicitly blocked for this stage.
- Backward compatibility checked for existing collectors, exporters,
  dashboards and automation consumers.
- Error responses and status codes are not changed accidentally.

## UI / runtime impact

- PR states whether UI impact is none, documentation-only or user-visible.
- PR states whether runtime deployment impact is none or requires operator
  action.
- No runtime behavior is changed by documentation/governance-only PRs.
- No service restart, migration or production config change is implied unless
  explicitly documented.

## Registry-readiness impact

- GitHub Actions is public mirror validation only.
- Public CI, Coverage and Security workflows are not registry release evidence.
- Release evidence must be produced on the Russian build-runner.
- Russian Gitea remains the primary registry-readiness source contour.
- Do not claim completed Russian software registry submission.
- Do not claim FSTEC/FSB certification.
- Do not claim SIEM/DLP replacement.
- Do not mark restore test as completed while `restore_tested=false`.
- Do not mark `awatch-build-01` as ready until provisioning evidence exists.

## Secret / PII safety

- No secrets, tokens, passwords or private keys in code, docs, logs,
  screenshots or workflow output.
- No employee personal data, real user activity traces or unredacted customer
  identifiers.
- No customer IP addresses, internal hostnames, VPN details or private network
  topology.
- Demo data is synthetic or anonymized.
- Public secret scan is expected to pass before merge.

## Documentation impact

- README, `docs/PROJECT_STATUS_RU.md`, registry docs and operational runbooks
  are updated when claims, checks, workflows or procedures change.
- New claims are conservative and evidence-backed.
- Pending work remains marked as planned/pending until evidence exists.
- Public mirror wording remains separate from registry release evidence.

## Deployment / rollback impact

- PR states whether deployment action is required.
- Rollback path is documented for runtime or automation changes.
- Documentation-only PRs state that runtime/API/UI impact is unchanged.
- Changes to scripts include syntax checks and a clear operator failure mode.

## Smoke checks

- Run checks relevant to changed files.
- For documentation/governance updates, expected minimum checks are:
  `python3 scripts/public_secret_pattern_check.py`,
  `bash -n scripts/registry_readiness_check.sh`,
  `bash scripts/registry_readiness_check.sh`,
  `git diff --check`.
- For shell changes, `bash -n` is mandatory for changed shell scripts.
- For Rust/product changes, use
  `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md` as the default local validation
  contour.
- For operator-facing web, gateway, worktime reports or Grafana dashboards,
  browser smoke through the rendered pages is required in addition to API checks.

## Evidence requirements

- PR records commands run and results.
- Skipped checks include a concrete reason.
- Registry release evidence is not accepted from GitHub Actions alone.
- Russian build-runner release evidence must include logs, checksums and
  artifact manifest when that contour is ready.
- Restore test evidence must include separate-host restore notes and checksum
  verification before `restore_tested` changes from false.
