# Release Candidate Runbook

Этот документ описывает техническую сборку Release Candidate для AWatch-rus. RC-сборка нужна, чтобы одной воспроизводимой командой собрать проверенные артефакты, зафиксировать git commit, сформировать SBOM/manifest/checksums и сложить результат в отдельный каталог под конкретное имя кандидата.

Release Candidate не равен юридической готовности к подаче в реестр и не заменяет финальную процедуру релиза.

## Evidence pack

Финальная проверка RC-процесса для ветки `hardening/pilot-v1-defects-cleanup` зафиксирована в `docs/RC_EVIDENCE_PACK_PILOT_V1_RU.md`.

## Запуск

Команда выполняется из корня репозитория:

```bash
bash scripts/build_release_candidate.sh v1.0.2-rc1
```

Первый аргумент обязателен. Имя кандидата используется как имя каталога в `dist/release-candidate/`, поэтому скрипт требует начало с буквы или цифры и дальше принимает только буквы, цифры, точку, подчеркивание и дефис.

Перед сборкой рабочее дерево git должно быть чистым. Если есть незакоммиченные, staged или untracked файлы, скрипт завершится с ошибкой. Это защищает RC от незафиксированного состояния.

## Preflight

Перед полной RC-сборкой можно проверить локальные предпосылки без создания каталога release candidate и без запуска cargo build/test:

```bash
bash scripts/build_release_candidate.sh --preflight
```

Preflight проверяет наличие команд `git`, `cargo`, `bash`, `node`, `sha256sum`, наличие обязательных внутренних скриптов, а также то, что `dist/` игнорируется git. Этот режим не требует чистого git tree, не создает артефакты и не заменяет полную RC-сборку.

## Если проект лежит на USB/HDD mount

На локальном контуре проект может лежать под `/mnt/` или `/media/`. В таком случае cargo build artifacts в стандартном `adk-rust/target` могут падать на filesystem-ограничениях mount, например на `libsqlite3-sys` с `Operation not permitted`.

Рекомендуемый запуск для такого контура:

```bash
CARGO_TARGET_DIR=$HOME/.cache/aw-rus-hardening-target bash scripts/build_release_candidate.sh v1.0.2-rc1
```

Это не обход проверок. Все `cargo fmt`, `cargo test`, `cargo clippy`, `cargo build`, `quality-gate`, private-config guard, OpenAPI contract guard и SBOM generation продолжают выполняться. Меняется только место, куда cargo складывает build artifacts.

`dist/` по-прежнему не коммитится. Требование чистого git tree для настоящей RC-сборки также остается обязательным.

## Проверки

Скрипт выполняет обязательные проверки и сборку Rust workspace:

```bash
cargo fmt --manifest-path adk-rust/Cargo.toml --all -- --check
cargo test --manifest-path adk-rust/Cargo.toml --workspace
cargo clippy --manifest-path adk-rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo build --manifest-path adk-rust/Cargo.toml --workspace --release
bash scripts/quality-gate.sh
bash scripts/check_private_config_guard.sh
node scripts/check_portal_contract_sync.mjs
```

Если любая проверка падает, RC-сборка считается несостоявшейся.
Неполный output-каталог при ошибке удаляется, чтобы не смешивать частичные артефакты с валидной сборкой.

## Артефакты

Результат складывается в:

```text
dist/release-candidate/<RC_NAME>/
```

Для примера выше итоговый каталог будет:

```text
dist/release-candidate/v1.0.2-rc1/
```

В каталоге создаются:

- `git-commit.txt` - commit, из которого собран кандидат;
- `FILES.txt` - список файлов, покрытых итоговыми checksum, кроме самого `SHA256SUMS.txt`;
- `SHA256SUMS.txt` - SHA-256 для всех файлов каталога, кроме самого `SHA256SUMS.txt`;
- `sbom/` - SBOM-файлы, созданные существующим генератором `scripts/generate_release_sbom_v0_2.sh`;
- `RELEASE_ASSETS_MANIFEST-v0.2.json` и `SHA256SUMS-v0.2.txt` - manifest/checksums, которые формирует существующий SBOM generator.

Каталог `dist/` не предназначен для коммита в git.

## Проверка checksum

Для проверки итоговых checksum:

```bash
cd dist/release-candidate/v1.0.2-rc1
sha256sum -c SHA256SUMS.txt
```

Ожидаемый результат - `OK` для всех записей. Любая ошибка означает, что набор артефактов изменился после сборки или поврежден.

## Перед реальной подачей

Release Candidate подтверждает техническую воспроизводимость сборки, но перед реальной подачей все еще нужны:

- release tag;
- release-specific SBOM;
- license review;
- signed/checksummed artifacts;
- проверка отсутствия live/private data;
- финальные install/user/admin guide под конкретную версию.
