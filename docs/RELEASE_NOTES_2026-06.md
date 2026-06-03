# Release notes 2026-06

Release tag: `v1.0.1-public-review`

Release type: public expert-review/source package.

## Release-readiness v0.1 addendum

Добавлен отдельный слой готовности поставки:

- portal UI блок `Готовность системы`;
- signed readiness bundle с `sha256sums.txt.sig`;
- Prometheus/Grafana alerts по readiness и signature verification;
- SBOM profile `docs/SBOM_V0.1_RU.md`;
- release readiness акт `docs/RELEASE_READINESS_V0.1_RU.md`;
- architecture схема `docs/diagrams/release-readiness-v0.1.md`;
- обезличенные screenshots портала в `docs/screenshots/release-v0.1/`.

## Release-readiness v0.2 addendum

Добавлен слой коммерческого/release hardening:

- machine SBOM generation для CycloneDX и SPDX;
- CI self-test checksum/signature verifier;
- шаблон customer pilot acceptance act;
- документ network perimeter/pfSense как optional integration layer;
- требование подписанного Git tag и detached signature для release assets.

## Release-readiness v0.3 addendum

Добавлен audit package:

- сторонние компоненты: компонент / версия / лицензия / назначение / риск;
- модель безопасности: роли, trust boundaries, данные, хранение, доступ;
- позиционирование для реестра: операционный контроль и аналитика событий,
  без заявления SIEM/DLP/EDR как основного класса;
- чек-лист пилотного внедрения у заказчика.

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
