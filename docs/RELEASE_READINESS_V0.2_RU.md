# Release readiness v0.2

`release-readiness-v0.2` усиливает v0.1 в части коммерческого релиза и
подготовки к реестру российского ПО.

Дата фиксации: `2026-06-03`.

## 1. Что добавлено

| Блок | Файл |
|---|---|
| Machine SBOM generation | `scripts/generate_release_sbom_v0_2.sh` |
| Release asset verification | `scripts/verify_release_assets.sh` |
| CI checksum/signature self-test | `.github/workflows/release-assets.yml` |
| Pilot acceptance act | `docs/CUSTOMER_PILOT_ACCEPTANCE_RU.md` |
| pfSense perimeter positioning | `docs/NETWORK_PERIMETER_PFSENSE_RU.md` |

## 2. Machine SBOM as GitHub Release asset

Сгенерировать assets:

```bash
bash scripts/generate_release_sbom_v0_2.sh dist/release-v0.2
```

Ожидаемые файлы:

```text
dist/release-v0.2/sbom/cyclonedx-rust-v0.2.json
dist/release-v0.2/sbom/spdx-rust-v0.2.json
dist/release-v0.2/sbom/cargo-metadata-v0.2.json
dist/release-v0.2/sbom/cargo-tree-v0.2.txt
dist/release-v0.2/sbom/python-inputs-v0.2.txt
dist/release-v0.2/RELEASE_ASSETS_MANIFEST-v0.2.json
dist/release-v0.2/SHA256SUMS-v0.2.txt
```

Эти файлы публикуются как GitHub Release assets. Они не хранятся в tracked
source, потому что `dist/` является generated output.

## 3. Подпись release assets

Release manager подписывает checksum file detached signature:

```bash
openssl dgst -sha256 -sign <RELEASE_PRIVATE_KEY.pem> \
  -out dist/release-v0.2/SHA256SUMS-v0.2.txt.sig \
  dist/release-v0.2/SHA256SUMS-v0.2.txt
```

Проверка:

```bash
RELEASE_VERIFY_PUBLIC_KEY=<RELEASE_PUBLIC_KEY.pem> \
  bash scripts/verify_release_assets.sh dist/release-v0.2
```

## 4. Подписанный Git tag

Требование для финального релиза:

```bash
git tag -s release-readiness-v0.2 -m "release-readiness-v0.2"
git tag -v release-readiness-v0.2
git push origin release-readiness-v0.2
```

Tag должен быть подписан пользовательским ключом правообладателя/maintainer, а
не сторонним системным ключом пакетов ОС.

## 5. CI gate

Workflow:

```text
.github/workflows/release-assets.yml
```

Проверяет:

- генерацию CycloneDX/SPDX JSON;
- валидность JSON;
- self-test checksum/signature verifier;
- отрицательные сценарии checksum mismatch и missing signature.

## 6. Pilot acceptance

Шаблон:

```text
docs/CUSTOMER_PILOT_ACCEPTANCE_RU.md
```

Использовать для коммерческого пилота после установки и первичной настройки.
Публичная версия должна оставаться обезличенной.

## 7. pfSense perimeter

Документ:

```text
docs/NETWORK_PERIMETER_PFSENSE_RU.md
```

Ключевая позиция: pfSense является опциональным интеграционным слоем, а не
обязательной частью продукта DetMir/AWatch-rus.

## 8. Acceptance checklist

- SBOM assets сгенерированы.
- `SHA256SUMS-v0.2.txt` создан.
- `SHA256SUMS-v0.2.txt.sig` создан release private key.
- `scripts/verify_release_assets.sh` проходит с public key.
- Git tag подписан ключом maintainer.
- GitHub release содержит SBOM JSON/TXT, manifest, checksum и signature.
- `CUSTOMER_PILOT_ACCEPTANCE_RU.md` заполнен для пилотного заказчика.
- pfSense не описан как обязательный компонент продукта.
