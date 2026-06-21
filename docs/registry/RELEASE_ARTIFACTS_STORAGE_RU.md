# Release artifacts storage

Статус: planned / partially implemented. Required before registry submission:
yes.

## Целевая модель

Release artifacts должны храниться в российском контуре. GitHub Releases не
должны быть primary release storage для registry-readiness. Если GitHub
Releases используются, они могут быть только public mirror.

Возможные варианты:

- отдельный каталог на `awatch-build-01`;
- отдельный REG.RU VPS;
- российское S3-compatible хранилище;
- `releases.iri1968.dpdns.org` или `releases.awatch-rus.ru` в будущем.

## Обязательные файлы

- Source archive.
- Binary archive.
- `SHA256SUMS`.
- SBOM CycloneDX.
- SBOM SPDX.
- `cargo-metadata.json`.
- `cargo-tree.txt`.
- Smoke logs.
- Release evidence manifest.
- Release notes.
- Version output.
- Health/readiness output, если применимо.

## Evidence требования

Для каждого release candidate фиксировать:

- storage location;
- timestamp;
- release version;
- commit SHA или tag;
- checksum verification result;
- responsible person placeholder: `[ЗАПОЛНИТЬ ПРАВООБЛАДАТЕЛЕМ]`.

До выбора и проверки хранилища нельзя утверждать, что release artifact storage
полностью готов для подачи в реестр.
