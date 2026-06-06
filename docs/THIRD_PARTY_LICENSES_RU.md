# Third-party licenses audit package

Дата фиксации: `2026-06-03`.

Документ является audit-facing сводкой по сторонним компонентам AWatch-rus /
AWatch-rus. Полный исторический inventory ведется в корне репозитория:
[`THIRD_PARTY_LICENSES_RU.md`](../THIRD_PARTY_LICENSES_RU.md). Машинные SBOM
artifacts для релиза публикуются в GitHub Release assets.

## 1. Правило версии

Для Rust-компонентов версия берется из `Cargo.lock` и SBOM release asset.
Для внешних сервисов версия фиксируется в конкретном deployment/release profile:
install-kit, Ansible inventory, system packages или container image manifest.

Команда для release-readiness v0.2/v0.3:

```bash
bash scripts/generate_release_sbom_v0_2.sh dist/release-v0.2
```

## 2. Ключевые компоненты

| Компонент | Версия / источник версии | Лицензия | Назначение | Риск |
|---|---|---|---|---|
| Собственный код AWatch-rus | Git tag / commit release package | Apache-2.0 | Rust helpers, portal, readiness, deployment docs/scripts | Низкий при сохранении собственных прав и clean release history. |
| ActivityWatch | Фиксируется в install/deployment profile | MPL-2.0 | Endpoint/server telemetry и ActivityWatch API | Средний: учитывать MPL границы при изменении upstream-файлов. |
| Grafana OSS | Фиксируется в deployment profile | AGPL-3.0 для современных OSS версий | Dashboards и визуализация | Повышенный: явно описывать как внешний компонент, проверять obligations. |
| Prometheus | Фиксируется в deployment profile | Apache-2.0 | Metrics, alert rules, scraping | Низкий/средний: соблюдать notice/license требования. |
| InfluxDB / compatible TSDB | Фиксируется в deployment profile | Зависит от версии/редакции | Временные ряды AW/worktime/Grafana | Средний: фиксировать конкретный дистрибутив. |
| SQLite | System/lib version в runtime | Public domain/blessing style | Local state, readiness, DLP/worktime storage | Низкий. |
| Hayabusa | Фиксируется в optional forensic profile | AGPLv3; rules могут иметь отдельные лицензии | Offline DFIR/enrichment | Повышенный: optional module, не смешивать с ядром без аудита. |
| Ansible | Версия control node/deployment image | GPL-3.0-or-later | Installation/deployment automation | Средний: toolchain, не linked runtime. |
| PowerShell | Windows PowerShell / PowerShell 7 version | Компонент ОС или MIT для PowerShell Core | Windows collectors/deployment scripts | Низкий/средний: зависит от среды заказчика. |
| Playwright | Release tooling version | Apache-2.0 | Browser smoke/screenshots | Низкий: test/release tooling, не runtime ядро. |
| Telegram bot dependencies | Python environment profile | По pip SBOM конкретного профиля | Уведомления/operator workflow | Средний: Telegram runtime остается Python-исключением. |
| Pollinations/OpenAI-compatible API integrations | External API terms | API terms, не OSS license | Optional AI helper/summaries | Средний: не включать ключи/API credentials в release. |

## 3. Rust crates из SBOM v0.2

| Crate | Версия | Лицензия | Назначение | Риск |
|---|---:|---|---|---|
| `anyhow` | `1.0.102` | MIT OR Apache-2.0 | Error handling | Низкий. |
| `clap` | `4.6.1` | MIT OR Apache-2.0 | CLI parsing | Низкий. |
| `chrono` | `0.4.44` | MIT OR Apache-2.0 | Date/time | Низкий. |
| `serde` | `1.0.228` | MIT OR Apache-2.0 | Serialization | Низкий. |
| `serde_json` | `1.0.150` | MIT OR Apache-2.0 | JSON | Низкий. |
| `serde_yaml` | `0.9.34+deprecated` | MIT OR Apache-2.0 | YAML config | Средний: crate deprecated, контролировать replacement path. |
| `reqwest` | `0.12.28` | MIT OR Apache-2.0 | HTTP client | Низкий/средний: проверить TLS transitive deps. |
| `rusqlite` | `0.32.1` | MIT | SQLite access | Низкий. |
| `regex` | `1.12.3` | MIT OR Apache-2.0 | Matching/rules | Низкий. |
| `sha2` | `0.10.9` | MIT OR Apache-2.0 | Hashing/checksums | Низкий. |
| `base64` | `0.22.1` | MIT OR Apache-2.0 | Encoding | Низкий. |
| `tiny_http` | `0.12.0` | MIT OR Apache-2.0 | Lightweight HTTP services | Низкий. |
| `url` | `2.5.8` | MIT OR Apache-2.0 | URL parsing | Низкий. |
| `tempfile` | `3.27.0` | MIT OR Apache-2.0 | Tests/temp files | Низкий, dev/test. |

## 4. Python и legacy exceptions

| Область | Версия / источник версии | Лицензия | Назначение | Риск |
|---|---|---|---|---|
| Telegram runtime | `requirements.txt`/venv заказчика | По pip SBOM | Telegram operator path | Средний: остается Python, но не ядро продукта. |
| OCR/content analysis | Python/system packages | По pip/dpkg SBOM | Optional OCR/text enrichment | Средний: Tesseract/Pillow/system deps проверить отдельно. |
| 1C/ETL/AI helpers | Python requirements конкретного профиля | По pip SBOM | Business data integration | Средний: отдельный прикладной слой. |
| Legacy fallbacks | Source tree, не primary runtime | По соответствующим deps | Rollback/reference behavior | Низкий/средний: явно маркировать как fallback. |

## 5. Release acceptance

Для audit package считается обязательным:

- наличие CycloneDX/SPDX SBOM как GitHub Release assets;
- `SHA256SUMS-v0.2.txt` и `SHA256SUMS-v0.2.txt.sig`;
- public key для проверки подписи;
- отсутствие live secrets/inventory/evidence в release assets;
- отдельная фиксация внешних сервисов по deployment profile заказчика.
