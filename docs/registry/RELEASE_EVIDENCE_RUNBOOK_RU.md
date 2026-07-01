# Release evidence runbook

Статус: registry-readiness runbook. Документ описывает выпуск release
candidate на российском build-runner `awatch-build-01`. Он не утверждает, что
release evidence уже production-ready до первого успешного запуска.

GitHub Actions is public mirror validation only. Public CI, coverage and
security workflows improve transparency, but registry release evidence must be
produced on the Russian build-runner.

## Процесс release candidate

1. Checkout конкретного commit SHA или tag.
2. Clean build environment.
3. Выполнить `cargo fmt --all --check`.
4. Выполнить `cargo test --workspace`.
5. Выполнить `cargo clippy --workspace --all-targets -- -D warnings`.
6. Выполнить `cargo build --workspace --release`.
7. Выполнить `bash scripts/registry_readiness_check.sh`.
8. Выполнить release evidence check:
   `bash scripts/check_release_evidence.sh <evidence-dir>`.
9. Запустить доступные smoke-тесты:
   `node scripts/deployment-readiness-smoke.mjs`,
   `node scripts/pilot-validation-smoke.mjs`,
   `node scripts/browser-conformance-smoke.mjs`, если не требует live stand,
   `bash scripts/validate_install_kit.sh`, если применимо.
10. Сформировать source archive.
11. Сформировать binary artifacts archive.
12. Сформировать `SHA256SUMS`.
13. Сформировать `cargo-metadata.json`.
14. Сформировать `cargo-tree.txt`.
15. Сформировать SBOM CycloneDX/SPDX, если tools доступны.
16. Сформировать release evidence manifest.
17. Сохранить logs.
18. Сохранить final report.

Build provenance requirements:

- `RELEASE_COMMIT` may be a SHA or tag, but manifest `release_commit` must be
  the resolved full 40-character Git SHA.
- `GIT_COMMIT` must be exported during Rust build so `/version` and other
  build/version probes can report the source revision.
- `SOURCE_DATE_EPOCH` defaults to the resolved commit timestamp and must be
  recorded in release evidence.
- `BUILD_TIME` must be derived from `SOURCE_DATE_EPOCH`, not from wall-clock
  build time.
- Wall-clock `generated_at` is evidence generation time only; it is not the
  embedded build timestamp.

Если smoke требует live stand, не удалять и не скрывать его. В release report
фиксировать: `skipped: requires live stand`.

## Автоматизация

Основной скрипт:

```bash
RELEASE_VERSION=registry-candidate-YYYYMMDD-HHMMSS \
RELEASE_COMMIT=<commit-sha-or-tag> \
OUTPUT_DIR=release-evidence/<release-version> \
bash scripts/build_release_evidence.sh
```

Для documentation-only изменения допустимо:

```bash
DOCS_ONLY=1 bash scripts/build_release_evidence.sh
```

`DOCS_ONLY=1` должен явно попасть в report. Это не должно использоваться для
product/runtime release candidate.

## Обязательные результаты

- `release-evidence-manifest.json`.
- `RELEASE_EVIDENCE_REPORT_RU.md`.
- `SHA256SUMS`.
- `logs/`.
- `logs/source-date-epoch.log`.
- `logs/build-time-utc.log`.
- Source archive.
- Binary artifacts archive или documented skip только для `DOCS_ONLY=1`.
- `cargo-metadata.json` или documented skip.
- `cargo-tree.txt` или documented skip.

## Conservative claims

Release evidence не является подтверждением юридически завершенной регистрации
в реестре РФ. Перед официальной подачей требуются rightsholder confirmation,
финальная юридическая проверка и проверка фактического build-runner.
