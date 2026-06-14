# GitHub-сборка Rust-бинарников AWatch-rus

## Принятое решение

Для проекта AWatch-rus каноническая release-сборка Rust-бинарников выполняется в GitHub Actions.

Локальная сборка используется для разработки и предварительной проверки. Официальным источником release-бинарников считаются только artifacts, полученные из GitHub Actions на конкретном commit или tag.

## Toolchain

Версия Rust/Cargo фиксируется в `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.94.0"
profile = "minimal"
components = ["rustfmt", "clippy"]
```

Workflow должны запускать Cargo явно:

```bash
cargo +1.94.0 --version
rustc +1.94.0 --version
cargo +1.94.0 fmt --manifest-path adk-rust/Cargo.toml --all -- --check
cargo +1.94.0 test --manifest-path adk-rust/Cargo.toml --workspace --no-fail-fast
cargo +1.94.0 clippy --manifest-path adk-rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo +1.94.0 build --manifest-path adk-rust/Cargo.toml --workspace --release
```

Это исключает ситуацию, когда GitHub runner использует старый системный Cargo.

## Workflow

Основные workflow:

- `.github/workflows/rust-workspace.yml` — fmt, tests, clippy, release build всего workspace.
- `.github/workflows/rust-professionalization-check.yml` — PR smoke для изменяемых Rust-крейтов.
- `.github/workflows/rust-binary-build.yml` — сборка release-бинарников Linux x86_64 и публикация GitHub Actions artifact.

## rust-binary-build

Workflow `rust-binary-build` запускается:

- вручную через GitHub Actions -> rust-binary-build -> Run workflow;
- автоматически при push tag вида `v*`.

Внутри workflow выполняется:

1. checkout repository;
2. установка Rust/Cargo 1.94.0;
3. вывод версий `cargo` и `rustc`;
4. format check;
5. workspace tests;
6. workspace clippy;
7. workspace release build;
8. upload artifact `awatch-rus-linux-x86_64-release-binaries`.

## Правило проекта

Перед передачей бинарников на пилот, демонстрацию или релиз нужно использовать GitHub Actions artifact, а не локально собранный файл.

Минимальные признаки корректного artifact:

- workflow завершился успешно;
- в логах указан Rust/Cargo 1.94.0;
- build выполнен из нужного commit или tag;
- artifact скачан из GitHub Actions.

## Дальнейшие улучшения

Отдельными PR можно добавить:

- SHA256SUMS для каждого бинарника;
- автоматическую публикацию в GitHub Release при tag `v*`;
- Windows x86_64 build для endpoint-компонентов;
- Linux static/musl build при необходимости;
- подпись release artifacts.
