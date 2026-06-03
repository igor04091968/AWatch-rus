# SBOM v0.1

Этот документ является human-readable SBOM profile для
`release-readiness-v0.1`. Машинные SBOM artifacts должны формироваться из
проверенного commit и публиковаться как release assets.

## 1. Идентификатор

| Поле | Значение |
|---|---|
| Product | DetMir, программный комплекс AWatch-rus |
| SBOM profile | v0.1 |
| Date | 2026-06-03 |
| Scope | Source + Rust workspace + Python exceptions + Node/Playwright tooling + OS/service dependencies |

## 2. Основные компоненты

| Компонент | Роль | Тип зависимости |
|---|---|---|
| ActivityWatch | Сбор и хранение активности | upstream application |
| aw-server-rust | ActivityWatch server runtime | upstream/Rust |
| DetMir Rust crates | status, checks, portal, readiness, DLP/worktime helpers | собственный код |
| Grafana | Dashboards и визуализация | third-party application |
| Prometheus | Metrics и alert rules | third-party application |
| Hayabusa | Offline forensic/security analytics | third-party tool |
| Ansible | Deployment automation | third-party tool |
| Telegram bot runtime | Оповещения и operator workflow | Python exception |
| Playwright | Browser smoke/screenshots | test/release tooling |

## 3. Rust SBOM input

Команды генерации:

```bash
mkdir -p dist/sbom
cargo metadata --manifest-path adk-rust/Cargo.toml --format-version 1 \
  > dist/sbom/cargo-metadata-v0.1.json
(cd adk-rust && cargo tree --workspace) \
  > dist/sbom/cargo-tree-v0.1.txt
```

Рекомендуемый CycloneDX:

```bash
cargo install cargo-cyclonedx --locked
cargo cyclonedx --manifest-path adk-rust/Cargo.toml \
  --format json --output-cdx dist/sbom/cyclonedx-rust-v0.1.json
```

## 4. Python SBOM input

Python остается как исключение, а не ядро продукта:

- Telegram runtime;
- legacy fallbacks;
- MCP/AI/ETL helpers;
- installer/release utilities.

Команды:

```bash
find aw-server clickhouse-1c detmir-mcp scripts -maxdepth 4 -type f \
  \( -name 'requirements.txt' -o -name 'pyproject.toml' -o -name 'setup.py' \) \
  -print -exec sha256sum {} \; > dist/sbom/python-inputs-v0.1.txt
```

Если используется virtualenv:

```bash
python3 -m pip freeze > dist/sbom/python-freeze-v0.1.txt
```

## 5. Node/Playwright SBOM input

Node используется для browser smoke и release screenshots.

```bash
npm ls --all --json > dist/sbom/npm-tree-v0.1.json
```

Если проект не содержит production Node runtime, этот artifact помечается как
`test/release tooling only`.

## 6. OS/service inventory

Для эталонной Debian/Ubuntu VM:

```bash
dpkg-query -W -f='${Package}\t${Version}\n' \
  > dist/sbom/debian-packages-v0.1.tsv
systemctl list-unit-files 'detmir-*' 'aw-*' --no-pager \
  > dist/sbom/systemd-units-v0.1.txt
```

## 7. License inventory

Основной документ:

```text
THIRD_PARTY_LICENSES_RU.md
```

Дополнительные checks:

```bash
cargo install cargo-deny --locked
(cd adk-rust && cargo deny check licenses)
```

## 8. Критерии приемки SBOM v0.1

- SBOM artifacts сформированы из того же commit, что и release package.
- SBOM не содержит secrets, live IPs, live domains, user home paths или
  runtime evidence.
- Для каждого крупного компонента понятны роль, upstream и license source.
- Python обозначен как исключение/интеграционный слой, не как ядро продукта.
- Машинные SBOM JSON/TXT публикуются в GitHub Release assets, а не обязательно
  хранятся в tracked source.
