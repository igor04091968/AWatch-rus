# Release notes 2026-06

Release tag: `v1.0.1-public-review`

Release type: public expert-review/source package.

## Что входит

- Обезличенный public source package DetMir/AWatch-rus.
- Документы для экспертной проверки и реестровой подготовки.
- Пересобранный sanitized install-kit.
- SBOM inputs для Rust/Python dependency review.
- SHA-256 checksums для release artifacts.

## Главные документы

- `REGISTER_RU_SOFTWARE.md`
- `PRODUCT_DESCRIPTION_RU.md`
- `THIRD_PARTY_LICENSES_RU.md`
- `docs/INSTALL_FOR_EXPERT_RU.md`
- `docs/EXPERT_TEST_SCENARIO_RU.md`
- `docs/SBOM_RELEASE_CHECKLIST_RU.md`
- `docs/RELEASE_AUDIT_2026-06.md`
- `docs/RELEASE_MANIFEST_2026-06.md`

## Что проверено

- Rust workspace собран в release mode.
- Все ожидаемые DetMir Rust release binaries найдены.
- Install-kit пересобран и прошел manifest/archive validation.
- Quality gate прошел.
- Публичные/private маркеры старого стенда не найдены в release-facing
  tracked surface.

## Release assets

- `install-kit-awindows-20260427-211240.zip`
- `install-kit-awindows-20260427-211240.tar.gz`
- `SHA256SUMS-2026-06.txt`
- `cargo-metadata-2026-06.json`
- `cargo-tree-2026-06.txt`
- `python-inputs-2026-06.txt`

## Ограничения

- Этот release не публикует приватную runtime-конфигурацию DetMir.
- Runtime-секреты, inventory, реальные домены, реальные адреса и evidence
  остаются вне Git и вне release assets.
- Telegram runtime остается на Python как принятое архитектурное исключение.
- pfSense runtime не менялся.

## Проверка экспертом

1. Установить экземпляр по `docs/INSTALL_FOR_EXPERT_RU.md`.
2. Пройти ручной сценарий `docs/EXPERT_TEST_SCENARIO_RU.md`.
3. Сверить checksums из `SHA256SUMS-2026-06.txt`.
4. Проверить audit `docs/RELEASE_AUDIT_2026-06.md`.
