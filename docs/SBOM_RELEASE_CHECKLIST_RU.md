# SBOM и release checklist

Документ фиксирует минимальный порядок подготовки публичного релиза
DetMir/AWatch-rus: исходники, сборка, артефакты, сторонние компоненты,
лицензии, публичная гигиена и экспертная проверка.

## 1. Идентификация релиза

Перед сборкой зафиксировать:

- product name: `DetMir, программный комплекс AWatch-rus`;
- repository: `AWatch-rus`;
- release tag: `vX.Y.Z`;
- commit: вывод `git rev-parse HEAD`;
- дата сборки;
- ответственный правообладатель/maintainer;
- состав release assets.

Команды:

```bash
git status --short
git rev-parse HEAD
git tag --points-at HEAD
```

Критерий:

- рабочее дерево не содержит незапланированных tracked-изменений;
- tag указывает на проверенный commit;
- release notes соответствуют фактическому составу поставки.

## 2. Публичная гигиена репозитория

Проверить отсутствие приватных идентификаторов в публичных документах:

```bash
PRIVATE_MARKERS_REGEX='<PRIVATE_HOSTNAME>|<PRIVATE_PUBLIC_DOMAIN>|<LOCAL_OPERATOR_HOME>|<ROOT_PRIVATE_PATH>'
git grep -n -E "$PRIVATE_MARKERS_REGEX" -- \
  README.md docs REGISTER_RU_SOFTWARE.md PRODUCT_DESCRIPTION_RU.md \
  SECURITY_OVERVIEW_RU.md adk-rust/RUNBOOK.md || true
```

Проверить реальные IP в release-facing документах:

```bash
git grep -n -E '10\\.10\\.10\\.|192\\.168\\.' -- \
  README.md docs REGISTER_RU_SOFTWARE.md PRODUCT_DESCRIPTION_RU.md \
  SECURITY_OVERVIEW_RU.md adk-rust/RUNBOOK.md || true
```

Критерий:

- в документах нет приватных IP, доменов, usernames и live hostnames;
- допускаются только placeholders: `<AW_SERVER_HOST>`, `<GRAFANA_HOST>`,
  `<PUBLIC_GATEWAY_FQDN>`, `HOST-EXAMPLE`, `WINDOWS_USER_EXAMPLE`;
- приватные configs находятся только в ignored-файлах.

## 3. Контроль секретов

Проверить, что в репозиторий не попали secrets:

```bash
git ls-files | grep -E '(^|/)secrets(/|$)|\\.env$|inventory\\.ini$' || true
git grep -n -E 'sk_[A-Za-z0-9]|pk_[A-Za-z0-9]|BEGIN OPENSSH PRIVATE KEY|password\\s*=' -- . || true
```

Рекомендуемый дополнительный контроль:

```bash
gitleaks detect --source . --no-git --redact
```

Критерий:

- директория `secrets/` не отслеживается Git;
- `.env`, `inventory.ini`, tokens, passwords и private keys отсутствуют в
  tracked-файлах;
- если секрет ранее был опубликован, он ротируется вне этого checklist.

## 4. Сборка Rust workspace

Собрать Rust-компоненты:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace
cd ..
```

Критерий:

- formatting clean;
- tests green;
- clippy без warnings;
- release-бинарники собраны.

## 5. Проверка Python-исключений

Python в проекте допускается только как:

- Telegram runtime, который решено оставить на Python;
- совместимые legacy fallbacks;
- installer/ops helpers, пока они не являются ядром продукта;
- тестовые и migration utilities.

Проверка:

```bash
git ls-files '*.py'
```

Критерий:

- Python-файлы имеют понятную роль;
- ядро DetMir/AWatch-rus позиционируется как Rust-first;
- README/registry docs не обещают отсутствие Python там, где он еще остается.

## 6. Third-party license inventory

Основной license inventory:

- [`../THIRD_PARTY_LICENSES_RU.md`](../THIRD_PARTY_LICENSES_RU.md)

Проверить минимум:

- ActivityWatch components: MPL-2.0 или лицензии конкретных upstream parts;
- Grafana: AGPL-3.0 или актуальная лицензия используемой версии;
- Prometheus: Apache-2.0;
- Hayabusa: лицензия upstream и правила распространения;
- Ansible: GPL-3.0-or-later;
- Rust crates: по `cargo metadata`, `cargo about`, `cargo deny`;
- Python dependencies: по `pip-licenses` или lock-файлам;
- JavaScript/Node dependencies для Playwright/UI helpers: по `npm ls` и
  package metadata.

Критерий:

- для каждого крупного компонента указан upstream, license, роль и риск;
- copyleft-компоненты не скрыты;
- release notes не противоречат лицензиям.

## 7. SBOM

Сформировать машинные перечни зависимостей.

Rust:

```bash
cd adk-rust
cargo metadata --format-version 1 > ../sbom-cargo-metadata.json
cargo tree --workspace > ../sbom-cargo-tree.txt
cd ..
```

Python, если используется виртуальное окружение:

```bash
python3 -m pip freeze > sbom-python-freeze.txt
```

Node/Playwright, если используется frontend smoke tooling:

```bash
npm ls --all --json > sbom-npm-tree.json
```

OS packages на эталонной VM:

```bash
dpkg-query -W -f='${Package}\\t${Version}\\n' > sbom-debian-packages.tsv
```

Критерий:

- SBOM artifacts приложены к release assets или сохранены в build archive;
- SBOM не содержит секретов;
- SBOM соответствует проверяемому commit/tag.

## 8. Release assets

Собрать install-kit и бинарные артефакты только из проверенного commit:

```bash
scripts/rebuild_install_kit.sh
scripts/validate_install_kit.sh
```

Критерий:

- dated zip/tar.gz не лежат в корне tracked-репозитория;
- архивы публикуются как GitHub Release assets;
- checksum каждого asset зафиксирован.

Пример фиксации checksum:

```bash
sha256sum dist/* install-kit-awindows-*.zip install-kit-awindows-*.tar.gz \
  > SHA256SUMS
```

## 9. Документы для реестра российского ПО

Проверить наличие и актуальность:

- `REGISTER_RU_SOFTWARE.md`;
- `PRODUCT_DESCRIPTION_RU.md`;
- `THIRD_PARTY_LICENSES_RU.md`;
- `docs/INSTALL_FOR_EXPERT_RU.md`;
- `docs/ARCHITECTURE_RU.md`;
- `docs/ADMIN_GUIDE_RU.md`;
- `docs/OPERATOR_GUIDE_RU.md`;
- `docs/OWNERSHIP_RU.md`;
- `docs/REGISTRY_CHECKLIST_RU.md`.

Критерий:

- документы согласованы по названию продукта;
- не заявляются DLP/SIEM/EDR/XDR/сертифицированная СЗИ как основной класс;
- позиционирование: операционный контроль, технический аудит, автоматизация
  эксплуатации ИТ-инфраструктуры.

## 10. Экспертная установка

Проверить релиз на чистой VM по:

- [`INSTALL_FOR_EXPERT_RU.md`](INSTALL_FOR_EXPERT_RU.md)

Критерий:

- чистая VM проходит путь установка -> сборка -> проверка;
- ActivityWatch API отвечает;
- базовые DetMir checks работают;
- diagnostic bundle собран;
- инструкция воспроизводима без доступа к личному стенду разработчика.

## 11. GitHub metadata

Проверить публичную страницу репозитория:

- description заполнен;
- website/homepage заполнен;
- topics заполнены;
- license отображается;
- releases содержат бинарные/install-kit assets;
- root README не содержит приватных адресов и личных путей.

Критерий:

- проект выглядит как поставляемое ПО, а не как личный стенд;
- About/Topics/Website заполнены;
- license detected на GitHub.

## 12. Финальный gate перед публикацией

Команды:

```bash
git diff --check
git status --short
PRIVATE_MARKERS_REGEX='<PRIVATE_HOSTNAME>|<PRIVATE_PUBLIC_DOMAIN>|<LOCAL_OPERATOR_HOME>|<ROOT_PRIVATE_PATH>'
git grep -n -E "$PRIVATE_MARKERS_REGEX" -- \
  README.md docs REGISTER_RU_SOFTWARE.md PRODUCT_DESCRIPTION_RU.md \
  SECURITY_OVERVIEW_RU.md adk-rust/RUNBOOK.md || true
```

Критерий публикации:

- diff не содержит whitespace errors;
- tracked changes входят в один понятный commit;
- release-facing docs обезличены;
- install-kit artifacts вынесены в Release assets;
- SBOM/license checklist приложен или воспроизводим;
- tag создан только после прохождения проверок.
