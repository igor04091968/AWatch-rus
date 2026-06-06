# AWatch-rus: сторонние компоненты и лицензии

Статус документа: рабочий license inventory для подготовки поставки и
экспертной проверки. Документ не является юридическим заключением. Перед
коммерческой поставкой или подачей в реестр нужно выполнить полный
автоматизированный SBOM/license audit по конкретной release-сборке.

## 1. Собственный код проекта

Собственные компоненты `AWatch-rus`:

- Rust workspace `adk-rust/`;
- AWatch-rus status/check/auto/heal helpers;
- worktime exporters/API/bridge/autoheal;
- DLP server-side helpers;
- evidence API и portal helpers;
- Ansible playbooks и deployment automation;
- Windows PowerShell collectors/deployment scripts;
- ActivityWatch RU WebUI patches;
- Grafana dashboards проекта;
- install-kit tooling;
- документация.

Для собственного кода в корне репозитория указан `LICENSE`:

```text
Apache License 2.0
```

Это применимо только к собственным частям проекта. Сторонние компоненты
сохраняют свои лицензии.

## 2. Ключевые сторонние компоненты

| Компонент | Роль в продукте | Типовая лицензия upstream | Статус поставки | Комментарий для аудита |
|---|---|---|---|---|
| ActivityWatch | Базовый сбор и API событий активности | MPL-2.0 | Устанавливается/используется как внешний компонент | Weak copyleft на измененные MPL-файлы; модификации ActivityWatch нужно учитывать отдельно. |
| Grafana OSS | Dashboards и визуализация | AGPL-3.0 для современных версий Grafana OSS | Обычно внешний сервис/контейнер, не собственный код AWatch-rus | AGPL требует отдельной проверки модели распространения и сетевого использования. |
| Prometheus | Monitoring ecosystem, exporters, scrape model | Apache-2.0 | Внешний компонент при включении мониторинга | Совместим с Apache-поставкой при соблюдении notice/license требований. |
| InfluxDB / compatible TSDB | Хранилище временных рядов `aw_metrics` | Зависит от версии/дистрибутива | Внешний компонент | Зафиксировать конкретную версию в release notes. |
| Hayabusa | Offline/DFIR timeline и enrichment | AGPLv3; rules могут иметь Detection Rule License | Опциональный прикладной модуль расследования | Не позиционировать как ядро продукта; проверить obligations при включении в поставку. |
| Ansible | Deployment automation | GPL-3.0-or-later для Ansible core | Инструмент установки | Обычно не линкуется с кодом продукта; входит в toolchain. |
| PowerShell | Windows deployment/collectors runtime | MIT для PowerShell Core; Windows PowerShell как компонент ОС | Runtime/tooling | Уточнять окружение заказчика: Windows PowerShell или PowerShell 7. |
| SQLite | Local state/warehouse DB | Public domain/blessing style | Embedded/library/runtime | Обычно низкий license risk. |
| ClickHouse clients/tooling | 1C/file analytics integration | Зависит от клиента; ClickHouse server Apache-2.0 | Отдельный 1C/business-data слой | Не является обязательным ядром AWatch-rus. |
| OpenAI/Pollinations-compatible integrations | AI assistant/integration paths | API terms, не open-source license | Опционально | Не включать ключи/API credentials в поставку. |

## 3. Rust dependencies

Rust является основным runtime-слоем AWatch-rus. Точный список зависимостей должен
фиксироваться по `Cargo.lock` конкретного релиза.

Ключевые crates, используемые в workspace:

| Crate | Назначение | Типичные лицензии ecosystem | Действие перед релизом |
|---|---|---|---|
| `anyhow` | Error handling | MIT OR Apache-2.0 | Проверить через `cargo about`. |
| `clap` | CLI parsing | MIT OR Apache-2.0 | Проверить transitive deps. |
| `chrono` | Date/time | MIT OR Apache-2.0 | Зафиксировать версию. |
| `serde`, `serde_json`, `serde_yaml` | Serialization | MIT OR Apache-2.0 | Проверить YAML transitive deps. |
| `reqwest` | HTTP client | MIT OR Apache-2.0 | Проверить TLS backend и transitive deps. |
| `rusqlite` | SQLite access | MIT | Проверить bundled/system SQLite режим. |
| `regex` | Matching rules | MIT OR Apache-2.0 | Низкий риск. |
| `sha2` | Hashing | MIT OR Apache-2.0 | Низкий риск. |
| `base64` | Encoding/decoding | MIT OR Apache-2.0 | Низкий риск. |
| `tiny_http` | Lightweight HTTP service | MIT OR Apache-2.0 | Проверить версию. |
| `url`, `urlencoding` | URL handling | MIT OR Apache-2.0 | Проверить transitive deps. |
| `tempfile` | Tests/temp files | MIT OR Apache-2.0 | Test/dev dependency. |

Обязательные команды для release audit:

```bash
cargo install cargo-about cargo-deny cargo-auditable
cd adk-rust
cargo metadata --locked --format-version 1 > ../docs/sbom-cargo-metadata.json
cargo deny check
cargo about generate about.hbs > ../docs/licenses-rust.html
```

Если шаблон `about.hbs` отсутствует, его нужно добавить в release tooling или
использовать стандартный шаблон организации.

## 4. Python-зависимости

Python не является основным runtime-ядром AWatch-rus. Он остается для
согласованных вспомогательных направлений:

- Telegram bot runtime, если включен в экземпляре;
- OCR/content-analysis path;
- 1C/AI/ETL integration layer;
- MCP/dev helper tools;
- legacy-compatible scripts, не входящие в Rust-first server core.

Известные requirements:

| Файл | Назначение | Основные зависимости | License-audit действие |
|---|---|---|---|
| `aw-server/dlp-content-analysis/requirements.txt` | OCR/content analysis | `pytesseract`, `Pillow` | Проверить OCR stack и system Tesseract license отдельно. |
| `aw-server/dlp-case-management/requirements.txt` | Legacy/reference case API | `fastapi`, `uvicorn`, `pydantic` | Проверить, поставляется ли как runtime или только reference. |
| `aw-server/dlp-policy-engine/requirements.txt` | Legacy/reference policy API | `fastapi`, `uvicorn`, `pydantic` | Rust replacement должен быть primary runtime. |
| `aw-server/dlp-compliance/requirements.txt` | Legacy/reference reports | `requests` | Проверить, не входит ли в active runtime. |
| `aw-server/dlp-integrations/requirements.txt` | Legacy/reference integrations | `PyYAML` | Проверить статус после Rust migration. |
| `clickhouse-1c/ai/requirements.txt` | 1C/AI APIs | `fastapi`, `uvicorn`, `clickhouse-connect` | Отдельный business-data слой. |
| `clickhouse-1c/etl/requirements.txt` | 1C ETL | `clickhouse-connect`, `PyYAML`, `python-dateutil`, `openpyxl` | Отдельный ETL слой. |
| `detmir-mcp` | MCP helper | Python MCP stack | Не основное runtime-ядро продукта. |

Команды для Python license report:

```bash
python3 -m venv /tmp/detmir-license-audit
. /tmp/detmir-license-audit/bin/activate
python -m pip install -U pip pip-licenses
pip-licenses --from=mixed --format=markdown > docs/licenses-python.md
```

Команду нужно выполнять в окружении, где установлены зависимости конкретного
release profile.

## 5. Frontend, dashboards и browser tooling

| Компонент | Роль | License-audit действие |
|---|---|---|
| Grafana dashboards JSON | Собственные dashboards AWatch-rus | Входят в собственную поставку; проверить отсутствие embedded secrets/URLs. |
| ActivityWatch WebUI patches | Собственный overlay/patch слой | Учитывать MPL-2.0 границы ActivityWatch, если изменяются upstream файлы. |
| Playwright/browser smoke tooling | Проверки UI | Обычно dev/test dependency; не включать в runtime claim. |
| JavaScript snippets | WebUI patching/helper scripts | Проверить зависимости, если добавляются npm packages. |

## 6. Компоненты с повышенным вниманием

| Компонент | Причина внимания | Рекомендация |
|---|---|---|
| Grafana OSS | AGPL-3.0 для современных версий | В реестровой поставке описывать как внешний компонент или проверить obligations. |
| Hayabusa | AGPLv3 + отдельная лицензия rules | Держать как optional offline module; не смешивать с закрытым ядром без аудита. |
| Ansible | GPL toolchain | Описывать как инструмент установки, не как linked library продукта. |
| OCR/Tesseract stack | Несколько уровней зависимостей | Фиксировать конкретные пакеты ОС и Python packages. |
| Python legacy paths | Могут выглядеть как ядро | В документации указывать, что Rust-first runtime является основным. |

## 7. Что поставляется вместе с продуктом

В публичной поставке могут присутствовать:

- исходный код собственных модулей;
- шаблоны конфигурации;
- Ansible playbooks;
- Grafana dashboards;
- Windows collectors scripts;
- install-kit archives как GitHub Release assets;
- документация.

Не должны поставляться в публичном git:

- production inventory;
- реальные домены/IP конкретного экземпляра;
- пароли и токены;
- runtime базы данных;
- customer evidence;
- локальная история разработки;
- случайные binary archives в корне репозитория.

## 8. Release checklist по лицензиям

Перед каждым публичным release:

1. Собрать Rust SBOM по `Cargo.lock`.
2. Выполнить `cargo deny check`.
3. Сформировать Rust license report.
4. Сформировать Python license report для включенных profiles.
5. Проверить Grafana/Hayabusa/Ansible как внешние компоненты.
6. Проверить, что root репозитория не содержит случайных архивов сборки.
7. Проверить отсутствие secrets, private inventory, customer paths.
8. Зафиксировать версию ActivityWatch и способ ее установки.
9. Зафиксировать, какие optional modules включены в релиз.
10. Сохранить отчеты в release artifacts или `docs/licenses-*`.

## 9. Источники для проверки upstream лицензий

- ActivityWatch repository/license: `https://github.com/ActivityWatch/activitywatch`
- Grafana licensing: `https://grafana.com/licensing/`
- Grafana repository/license: `https://github.com/grafana/grafana`
- Prometheus repository/license: `https://github.com/prometheus/prometheus`
- Hayabusa repository/license: `https://github.com/Yamato-Security/hayabusa`
- Ansible repository/license: `https://github.com/ansible/ansible`

Финальная версия документа должна ссылаться на конкретные версии компонентов,
использованные в release build.
