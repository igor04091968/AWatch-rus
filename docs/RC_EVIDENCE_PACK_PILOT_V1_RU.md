# RC Evidence Pack: Pilot v1

Документ фиксирует доказательства финальной проверки release candidate процесса для ветки `hardening/pilot-v1-defects-cleanup`.

## Идентификаторы проверки

- Branch/ref: `origin/hardening/pilot-v1-defects-cleanup`
- Commit: `a8c0482e760cc17b53182999355f65c17457d7f2`
- Commit short: `a8c0482`
- Дата проверки: `2026-06-12`
- Clean worktree: `<LOCAL_VALIDATION_WORKTREE>/AWatch-rus-rc-validation-a8c0482`
- RC name: `v1.0.2-rc-validation`
- RC output: `dist/release-candidate/v1.0.2-rc-validation/`
- `CARGO_TARGET_DIR`: `$HOME/.cache/aw-rus-hardening-target`

Абсолютный путь локального операторского home-каталога намеренно не фиксируется в tracked-документации. Это не влияет на воспроизводимость: команда использует стандартный `$HOME`.

## Команды проверки

Preflight без вынесенного target dir:

```bash
bash scripts/build_release_candidate.sh --preflight
```

Preflight с вынесенным cargo target dir:

```bash
CARGO_TARGET_DIR=$HOME/.cache/aw-rus-hardening-target \
  bash scripts/build_release_candidate.sh --preflight
```

Полная RC-сборка:

```bash
CARGO_TARGET_DIR=$HOME/.cache/aw-rus-hardening-target \
  bash scripts/build_release_candidate.sh v1.0.2-rc-validation
```

Команда полной сборки без имени RC проверена отдельно и корректно завершается с `exit=2`, потому что первый аргумент обязателен.

## Созданные RC artifacts

В каталоге `dist/release-candidate/v1.0.2-rc-validation/` созданы:

- `FILES.txt`
- `SHA256SUMS.txt`
- `SHA256SUMS-v0.2.txt`
- `git-commit.txt`
- `RELEASE_ASSETS_MANIFEST-v0.2.json`
- `sbom/cargo-metadata-v0.2.json`
- `sbom/cargo-tree-v0.2.txt`
- `sbom/cyclonedx-rust-v0.2.json`
- `sbom/python-inputs-v0.2.txt`
- `sbom/spdx-rust-v0.2.json`

`git-commit.txt` содержит `a8c0482e760cc17b53182999355f65c17457d7f2`.

## Artifact verification

Подтверждено:

- `sha256sum -c SHA256SUMS.txt`: OK
- `sha256sum -c SHA256SUMS-v0.2.txt`: OK
- JSON parse для `RELEASE_ASSETS_MANIFEST-v0.2.json`: OK
- JSON parse для `sbom/cargo-metadata-v0.2.json`: OK
- JSON parse для `sbom/cyclonedx-rust-v0.2.json`: OK
- JSON parse для `sbom/spdx-rust-v0.2.json`: OK
- `FILES.txt` соответствует фактическому набору checksum-covered файлов: OK

Повторный запуск с тем же `RC_NAME` блокируется сообщением `release candidate output already exists`; существующий `SHA256SUMS.txt` не изменяется.

## Dirty-tree guard

Clean-tree requirement сохранен и проверен двумя сценариями:

- non-ignored untracked file блокирует настоящую RC-сборку;
- tracked modification блокирует настоящую RC-сборку.

В обоих случаях скрипт завершается до создания RC-каталога. Ignored files намеренно не блокируют сборку, иначе `dist/` ломал бы повторные проверки и локальную валидацию артефактов.

## dist/ и git

Подтверждено:

- `git ls-files dist` возвращает `0` tracked files;
- `git status --ignored dist` показывает `!! dist/`;
- `dist/` не добавляется в git и остается локальным output-каталогом.

## Обязательные проверки

В clean validation worktree выполнены:

- `bash -n scripts/build_release_candidate.sh`: OK
- `bash scripts/build_release_candidate.sh --preflight`: OK
- `CARGO_TARGET_DIR=$HOME/.cache/aw-rus-hardening-target bash scripts/build_release_candidate.sh --preflight`: OK
- `git diff --check`: OK
- `bash scripts/check_private_config_guard.sh`: OK
- `node scripts/check_portal_contract_sync.mjs`: OK
- `bash scripts/quality-gate.sh`: OK

Внутри полной RC-сборки также прошли:

- `cargo fmt --manifest-path adk-rust/Cargo.toml --all -- --check`
- `cargo test --manifest-path adk-rust/Cargo.toml --workspace`
- `cargo clippy --manifest-path adk-rust/Cargo.toml --workspace --all-targets -- -D warnings`
- `cargo build --manifest-path adk-rust/Cargo.toml --workspace --release`

## Вывод

Release candidate процесс подтвержден как воспроизводимый в чистом рабочем дереве. Clean-tree requirement сохранен. Сборка не требует ослабления защитных проверок. Ветка готова к review и merge в main.
