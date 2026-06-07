# Registry Dependency Statement

Документ фиксирует dependency boundary AWatch-rus для подготовки к будущей
подаче в реестр российского ПО.

Документ не заменяет юридическую проверку лицензий.

## Open-source Dependencies

AWatch-rus использует open-source компоненты и tooling:

- Rust crates from Cargo ecosystem;
- ActivityWatch-compatible components;
- Grafana/Prometheus/InfluxDB where included in deployment profile;
- Ansible for deployment automation;
- Node.js/Playwright for smoke and screenshot tooling;
- Python libraries for вспомогательные направления, если они используются в
  конкретном контуре.

Точный перечень должен фиксироваться для каждого release tag через SBOM.

## SBOM Assets

Связанные документы:

- [SBOM_RELEASE_CHECKLIST_RU.md](SBOM_RELEASE_CHECKLIST_RU.md);
- [SBOM_V0.1_RU.md](SBOM_V0.1_RU.md);
- [THIRD_PARTY_LICENSES_RU.md](THIRD_PARTY_LICENSES_RU.md);
- [../THIRD_PARTY_LICENSES_RU.md](../THIRD_PARTY_LICENSES_RU.md).

Для подачи или экспертной проверки нужно формировать release-specific SBOM:

- Cargo metadata;
- SPDX/CycloneDX where available;
- third-party license table;
- checksums for release assets;
- list of bundled/non-bundled dependencies.

## Cargo Dependencies

Rust workspace должен проверяться на уровне конкретного release commit:

```bash
cd adk-rust
cargo metadata --format-version 1
cargo tree --workspace
```

Рекомендуемые дополнительные проверки перед реальной подачей:

- `cargo about`;
- `cargo deny`;
- review of copyleft licenses;
- проверка transitive dependencies.

## SaaS Dependency Boundary

Core AWatch-rus не должен зависеть от внешнего SaaS для выполнения основной
серверной логики.

Внешние сервисы, если они используются в отдельном внедрении, должны быть
описаны как deployment-specific integrations, а не как обязательная зависимость
продукта.

## License Review Before Submission

Перед реальной подачей нужно проверить:

- права на собственные модули;
- license compatibility для ActivityWatch/Grafana/Prometheus/Hayabusa/Ansible;
- Rust crates licenses;
- Python and Node dependencies licenses;
- правила распространения install-kit;
- наличие NOTICE/ATTRIBUTION, если требуется;
- отсутствие embedded proprietary assets без права распространения.

## Current Gaps

Оставшиеся задачи до формальной подачи:

- зафиксировать release tag и release-specific SBOM;
- обновить third-party license inventory на точный состав release assets;
- провести юридическую проверку license compatibility;
- подготовить правообладательский пакет;
- при необходимости подготовить свидетельство о регистрации программы для ЭВМ.
