# Журнал изменений

## release-readiness-v0.2 - 2026-06-03

Назначение этапа: усилить пакет для коммерческого релиза и реестра российского
ПО за счет машинного SBOM, проверки release assets, пилотного акта приемки и
корректного позиционирования pfSense.

### Добавлено

- `scripts/generate_release_sbom_v0_2.sh` - генерация CycloneDX/SPDX SBOM
  artifacts в `dist/release-v0.2/`.
- `scripts/verify_release_assets.sh` - проверка `SHA256SUMS*.txt` и detached
  signature release assets.
- `.github/workflows/release-assets.yml` - CI self-test checksum/signature
  verifier и генерации SBOM.
- `docs/RELEASE_READINESS_V0.2_RU.md` - контрольная карта v0.2.
- `docs/CUSTOMER_PILOT_ACCEPTANCE_RU.md` - шаблон акта приемки пилота.
- `docs/NETWORK_PERIMETER_PFSENSE_RU.md` - pfSense как опциональный
  интеграционный слой, не обязательная часть продукта.

## release-readiness-v0.1 - 2026-06-03

Назначение этапа: довести DetMir/AWatch-rus до проверяемого release-readiness
пакета для пилота, экспертной оценки и последующей публикации релиза без
раскрытия приватного коммерческого контура.

### Добавлено

- UI-блок портала `Готовность системы`: статус `OK/WARN/FAIL`, дата bundle,
  статус checksum, статус detached signature, fingerprint публичного ключа и
  ручная кнопка проверки bundle.
- Prometheus/Grafana alert rules:
  `detmir_readiness_ok == 0` и
  `detmir_readiness_signature_verified == 0`.
- Подпись readiness bundle через detached signature `sha256sums.txt.sig`.
- Retention для readiness archives и unit-тесты на подпись/retention.
- `docs/RELEASE_READINESS_V0.1_RU.md` - сводный акт готовности релиза v0.1.
- `docs/SBOM_V0.1_RU.md` - human-readable SBOM profile и команды генерации
  машинных SBOM artifacts.
- `docs/PORTAL_SCREENSHOTS_RU.md` - перечень обезличенных screenshots портала.
- `docs/diagrams/release-readiness-v0.1.md` - схема release-readiness path.

### Проверено

- `cargo fmt --manifest-path adk-rust/Cargo.toml --all -- --check`.
- `cargo test --manifest-path adk-rust/Cargo.toml -p detmir-readiness -p detmir-portal`.
- `cargo clippy --manifest-path adk-rust/Cargo.toml -p detmir-readiness -p detmir-portal --all-targets -- -D warnings`.
- `node --check adk-rust/crates/detmir-portal/src/static/app.js`.
- Ansible syntax-check для AW server и DetMir portal deploy playbooks.
- Runtime deployment на проектные сервисы DetMir без изменения pfSense или
  Proxmox platform layer.

## v1.0.1-public-review - 2026-06-03

Назначение релиза: публичный пакет для экспертной оценки DetMir/AWatch-rus и
подготовки к реестровой проверке. Релиз не меняет работающий коммерческий
runtime DetMir; изменения относятся к source/release package, документации,
обезличиванию и проверяемости поставки.

### Добавлено

- `docs/INSTALL_FOR_EXPERT_RU.md` - воспроизводимая установка: чистая VM,
  сборка, установка, проверка, ожидаемый результат.
- `docs/EXPERT_TEST_SCENARIO_RU.md` - ручной сценарий экспертной проверки:
  вход в web UI, status, clipboard/USB/print, DLP incident, case/evidence,
  export report.
- `docs/SBOM_RELEASE_CHECKLIST_RU.md` - checklist подготовки SBOM/release.
- `docs/RELEASE_AUDIT_2026-06.md` - audit приватных маркеров и секретов.
- `docs/RELEASE_MANIFEST_2026-06.md` - manifest release artifacts, checksums,
  SBOM inputs и выполненных gates.
- `docs/RELEASE_NOTES_2026-06.md` - release notes для GitHub release.

### Изменено

- Публичные docs, examples, defaults и test fixtures обезличены: live hostnames,
  private IPs, operator domains, local operator home paths, private root paths,
  live case IDs и forensic paths заменены на placeholders/TEST-NET значения.
- README и register docs теперь ведут эксперта по полному маршруту:
  описание продукта -> установка -> ручной сценарий -> audit -> SBOM/release
  checklist.
- Install-kit пересобирается из sanitized source files и валидируется через
  Rust tooling.

### Удалено из tracked source

- `.planning` generated artifacts.
- Распакованный `install-kit-awindows-20260427-211240/` как tracked source.
  Install-kit archives публикуются как GitHub Release assets.

### Проверено

- `cargo build --release --workspace` в отдельном target-dir.
- `scripts/check_detmir_rust_release_artifacts.sh`: все Rust release binaries
  найдены.
- `scripts/rebuild_install_kit.sh` и `scripts/validate_install_kit.sh`:
  install-kit пересобран и валиден.
- `scripts/quality-gate.sh`: `OK`.
- Public hygiene grep по tracked release surface: старые приватные маркеры
  отсутствуют; оставшиеся root-word совпадения классифицированы как
  ложноположительные technical path terms.

### Известные ограничения

- `v1.0.1-public-review` является source/review release. Коммерческий runtime
  DetMir продолжает использовать private runtime config вне Git.
- Python остается для Telegram runtime, OCR/content-analysis, 1C/AI/ETL и MCP
  helpers. Это отражено в registry docs как допустимое исключение.
- pfSense/infrastructure runtime не менялся в рамках этого релиза.

## v1.0.0 - 2026-04-25

Базовый professional baseline с install-kit artifacts.
