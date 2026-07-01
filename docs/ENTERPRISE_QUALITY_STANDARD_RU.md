# AWatch-rus: enterprise quality standard

Дата актуализации: 2026-07-01.

AWatch-rus уже работает в production-контуре заказчика. Основная цель
разработки - не добавление функций, а повышение надежности, эксплуатационной
зрелости, безопасности, сопровождаемости и воспроизводимости.

Приоритеты изменений, по убыванию:

1. Reliability.
2. Operational maturity.
3. Security.
4. Maintainability.
5. Reproducibility.
6. Performance.
7. Simplicity.

Новые функции не должны иметь приоритет над стабильностью production.

## Production-first правила

- Предполагать, что production deployment существует и пользователи зависят от
  непрерывной работы.
- Предпочитать additive/backward-compatible изменения.
- Не перепроектировать работающие подсистемы без измеримой пользы.
- Не включать heavy DLP, Loki или always-on Velociraptor без отдельного
  operator-approved решения.
- Не менять ActivityWatch logical host ids, bucket suffixes, Grafana variables
  или ClickHouse workforce keys без отдельной compatibility-процедуры.
- Fail closed для security-sensitive и deployment-sensitive paths.
- Сохранять rollback path для runtime, automation, config и dependency changes.

## Порядок принятия решений

Перед реализацией любого изменения ответ должен быть положительным хотя бы на
один вопрос:

1. Улучшает ли это production stability?
2. Снижает ли это operational risk?
3. Улучшает ли это diagnostics или observability?
4. Улучшает ли это maintainability?
5. Уменьшает ли это технический долг с низким regression risk?

Если ответ отрицательный на все пять вопросов, изменение не должно попадать в
production-oriented PR.

## Dependencies

Перед добавлением зависимости нужно явно обосновать:

- почему стандартной библиотеки недостаточно;
- почему существующий workspace crate не решает задачу;
- operational cost зависимости;
- maintenance cost зависимости;
- license/security impact.

Неиспользуемые зависимости удаляются отдельными низкорисковыми PR после
targeted tests. Обновление `Cargo.lock` без причины не допускается.

## Required PR sections

Каждый PR обязан содержать:

- Purpose.
- Operational impact.
- Risk assessment.
- Rollback strategy.
- Validation steps.
- Documentation changes.
- Acceptance criteria.

Для documentation-only/governance-only PR нужно явно указать, что runtime, API и
UI impact отсутствуют.

## Validation baseline

Канонический набор проверок описан в
`docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`. Минимальный baseline для
существенных Rust/runtime changes:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target

cargo fmt --all --check
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo audit --deny warnings

cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
cargo deny --manifest-path adk-rust/Cargo.toml check \
  --config deny.toml \
  --hide-inclusion-graph \
  --show-stats
python3 scripts/public_secret_pattern_check.py
node scripts/operational-maturity-check.mjs --json
```

Targeted checks are acceptable for small docs/governance or isolated changes,
but skipped checks must have a concrete reason.

## Technical debt policy

Fix technical debt only when all conditions are true:

- change is isolated;
- regression risk is low;
- tests or operational smoke cover the touched behavior;
- documentation remains accurate.

Otherwise create a follow-up task or document the residual risk instead of
mixing broad cleanup into a functional PR.

## Canonical references

- Review checklist: `docs/REVIEW_CHECKLIST_RU.md`.
- Validation runbook: `docs/OPERATIONS_VALIDATION_RUNBOOK_RU.md`.
- Operational maturity harness: `docs/OPERATIONAL_MATURITY_RU.md`.
- PR workflow: `docs/PR_REVIEW_WORKFLOW_RU.md`.
- GitHub governance entrypoint: `.github/GOVERNANCE.md`.
