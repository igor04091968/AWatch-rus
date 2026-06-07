# Registry Readiness Checklist

Чеклист фиксирует, что уже подготовлено для registry-readiness, и какие
пробелы остаются до реальной подачи.

Статус: подготовительный чеклист, не юридическая гарантия.

## Исходный код

- [x] Репозиторий содержит исходный код.
- [x] Rust workspace присутствует.
- [x] Документация хранится в репозитории.
- [ ] Перед подачей нужен release tag, выбранный как baseline.
- [ ] Нужно проверить отсутствие приватных runtime artifacts в release assets.

## Сборка

- [x] Есть Rust build/test workflow на уровне локальных команд.
- [x] Есть install-kit validation tooling.
- [ ] Для подачи нужен воспроизводимый release build log.
- [ ] Нужны checksums для конкретных release artifacts.

## Документация

- [x] README содержит product boundary.
- [x] Есть install/admin/operator documentation.
- [x] Есть Pilot v1 docs.
- [x] Есть demo pack.
- [x] Есть registry-readiness docs.
- [ ] Перед подачей нужно привести публичный сайт/страницу продукта к
  актуальному release state.

## SBOM

- [x] Есть SBOM/release checklist.
- [x] Есть third-party license docs.
- [ ] Нужен release-specific SBOM for final tag.
- [ ] Нужна юридическая проверка license compatibility.

## Лицензии

- [x] Есть основной `LICENSE`.
- [x] Есть third-party license inventory.
- [ ] Нужна финальная сверка transitive dependencies.
- [ ] Нужны NOTICE/ATTRIBUTION files, если проверка лицензий потребует.

## Install Guide

- [x] Есть [INSTALL_RU.md](INSTALL_RU.md).
- [x] Есть [FULL_DEPLOYMENT_MANUAL_RU.md](FULL_DEPLOYMENT_MANUAL_RU.md).
- [x] Есть deployment strategy docs.
- [ ] Нужно зафиксировать профиль установки для конкретной версии подачи.

## User / Admin Guides

- [x] Есть [ADMIN_GUIDE_RU.md](ADMIN_GUIDE_RU.md).
- [x] Есть [OPERATOR_GUIDE_RU.md](OPERATOR_GUIDE_RU.md).
- [x] Есть demo runbook.
- [ ] Для подачи нужен финальный user-facing guide без внутренних runtime
  примечаний.

## Demo Pack

- [x] Есть demo dataset.
- [x] Есть role-based demo scenarios.
- [x] Есть demo screenshots.
- [x] Есть demo report example.
- [x] Есть pilot value proposition.

## Release Assets

- [x] Есть release/install-kit tooling.
- [ ] Нужен финальный release package для конкретного tag.
- [ ] Нужны checksums.
- [ ] Нужен manifest of included files.
- [ ] Нужно подтвердить, что release assets не содержат secrets/live data.

## Screenshots

- [x] Есть GitHub demo screenshots.
- [x] Скриншоты подготовлены на demo data.
- [ ] Перед подачей нужно проверить актуальность screenshots относительно
  финального release UI.

## Functional Description

- [x] Есть product positioning.
- [x] Есть functional scope.
- [x] Есть registry product passport.
- [x] Есть architecture description.
- [x] Есть commercial positioning.

## Ограничения

- [x] Полноценная DLP не заявляется.
- [x] Полноценная SIEM не заявляется.
- [x] EDR/XDR не заявляется.
- [x] ML/LLM scoring не заявляется.
- [x] pfSense описан как optional addon / `contract_only`, если ingestion не
  включен и не принят отдельно.
- [x] React/Tauri описаны как future roadmap, не текущий claim.

## Остаточные пробелы до реальной подачи

- правообладательский пакет;
- release tag and signed/checksummed artifacts;
- release-specific SBOM;
- юридическая проверка лицензий;
- публичная страница продукта с документацией;
- финальный install/user/admin guide под конкретный release;
- подтверждение модели распространения и поддержки;
- проверка требований актуальной редакции правил реестра.
