# DetMir: сторонние компоненты и лицензии

Статус: первичный inventory для подготовки к реестру российского ПО.

Этот документ не заменяет юридическую license audit. Перед подачей в реестр
нужно выполнить автоматизированную проверку зависимостей и сохранить отчет.

Продуктовое имя: `DetMir`.

Техническая база и репозиторий: `AWatch-rus`.

## 1. Собственный код

Собственный код проекта включает:

- Rust workspace `adk-rust/`;
- Ansible playbooks;
- Windows PowerShell collectors и deployment scripts;
- AW-rus WebUI patches;
- DetMir Portal;
- DLP/worktime/status/reporting helpers;
- документацию проекта.

В `adk-rust/Cargo.toml` для workspace указан license:

```text
Apache-2.0
```

Перед публичной поставкой нужно проверить, что license root файла и все
исходники согласованы с выбранной моделью поставки.

## 2. Основные внешние компоненты runtime

| Компонент | Роль |
|---|---|
| ActivityWatch | Базовый сбор и API событий активности. |
| Grafana | Dashboards и визуализация. |
| InfluxDB | Метрики и временные ряды. |
| ClickHouse | 1C/file analytics слой. |
| SQLite | Локальные state/warehouse/cases/policy хранилища. |
| Python | Telegram runtime и legacy-compatible tooling. |
| Rust crates ecosystem | Основной runtime DetMir helpers. |
| PowerShell | Windows collectors/deployment. |
| Hayabusa | Offline/DFIR enrichment. |
| Ansible | Deployment automation. |

## 3. Rust зависимости

Найдены workspace dependencies:

| Dependency | Использование |
|---|---|
| `adk-rust` | Agent/tooling integration. |
| `anyhow` | Error handling. |
| `base64` | Evidence upload body decoding. |
| `chrono` | Time/date handling. |
| `clap` | CLI parsing. |
| `fs2` | File locks. |
| `reqwest` | HTTP client. |
| `regex` | Rules/content matching. |
| `rusqlite` | SQLite access. |
| `serde`, `serde_json`, `serde_yaml` | Serialization. |
| `sha2` | SHA-256 validation. |
| `tempfile` | Tests/temp files. |
| `tiny_http` | Lightweight HTTP services. |
| `url`, `urlencoding` | URL handling. |

Перед релизом выполнить:

```bash
cargo install cargo-about cargo-deny
cd adk-rust
cargo about generate about.hbs > ../docs/licenses-rust.html
cargo deny check
```

Шаблон `about.hbs` нужно добавить отдельно.

## 4. Python зависимости

Найдены requirements:

| Файл | Зависимости |
|---|---|
| `aw-server/dlp-case-management/requirements.txt` | `fastapi`, `uvicorn`, `pydantic` |
| `aw-server/dlp-compliance/requirements.txt` | `requests` |
| `aw-server/dlp-content-analysis/requirements.txt` | `pytesseract`, `Pillow` |
| `aw-server/dlp-integrations/requirements.txt` | `PyYAML` |
| `aw-server/dlp-policy-engine/requirements.txt` | `fastapi`, `uvicorn`, `pydantic` |
| `clickhouse-1c/ai/requirements.txt` | `fastapi`, `uvicorn`, `clickhouse-connect` |
| `clickhouse-1c/etl/requirements.txt` | `clickhouse-connect`, `PyYAML`, `python-dateutil`, `openpyxl` |

Перед релизом выполнить license scan:

```bash
python3 -m pip install pip-licenses
pip-licenses --from=mixed --format=markdown > docs/licenses-python.md
```

Команду запускать в воспроизводимом virtualenv, где установлены реальные
runtime dependencies.

## 5. Что нужно закрыть перед подачей

1. Добавить root `LICENSE`.
2. Добавить `NOTICE`, если это потребуется выбранными лицензиями.
3. Сформировать полный SBOM.
4. Проверить transitive dependencies.
5. Зафиксировать список компонентов, которые поставляются вместе с продуктом.
6. Отделить компоненты, которые устанавливаются пользователем самостоятельно.
7. Проверить отсутствие GPL/AGPL-компонентов, если выбранная модель поставки с
   ними несовместима.
8. Подготовить публичный `THIRD_PARTY_NOTICES`.

## 6. Связанные документы

- `docs/OWNERSHIP_RU.md`
- `docs/REGISTRY_CHECKLIST_RU.md`
- `docs/DETMIR_RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md`
