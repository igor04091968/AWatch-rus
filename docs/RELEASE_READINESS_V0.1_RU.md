# Release readiness v0.1

Документ фиксирует готовность AWatch-rus к первому управляемому release
readiness циклу: проверяемость сборки, SBOM, installation/runbook, архитектура,
портал, readiness bundle и публичная гигиена.

Дата фиксации: `2026-06-03`.

## 1. Назначение

`release-readiness-v0.1` не является отдельным коммерческим SKU и не заменяет
GitHub release tag. Это контрольная точка готовности поставки перед пилотом,
экспертной оценкой и публикацией release assets.

Цели:

- показать, что контур проверяется одной readiness-командой;
- подтвердить подпись и checksum readiness bundle;
- зафиксировать SBOM/third-party inventory;
- дать эксперту повторяемую установку и ручной сценарий проверки;
- приложить обезличенные screenshots портала;
- сохранить разделение public source и live commercial AWatch-rus runtime.

## 2. Состав release-readiness пакета

| Блок | Файл/артефакт | Статус |
|---|---|---|
| Changelog | `CHANGELOG_RU.md` | готов |
| Release notes | `docs/RELEASE_NOTES_2026-06.md` | готов |
| Manifest | `docs/RELEASE_MANIFEST_2026-06.md` | готов |
| SBOM profile | `docs/SBOM_V0.1_RU.md` | готов |
| SBOM checklist | `docs/SBOM_RELEASE_CHECKLIST_RU.md` | готов |
| Installation | `docs/INSTALL_FOR_EXPERT_RU.md`, `docs/INSTALL_RU.md` | готов |
| Runbook | `adk-rust/RUNBOOK.md`, `docs/runbook.md` | готов |
| Architecture | `docs/ARCHITECTURE_RU.md`, `docs/diagrams/release-readiness-v0.1.md` | готов |
| Portal screenshots | `docs/PORTAL_SCREENSHOTS_RU.md`, `docs/screenshots/release-v0.1/*.png` | готов |
| Readiness docs | `docs/PRODUCTION_READINESS_RU.md` | готов |
| Public audit | `docs/RELEASE_AUDIT_2026-06.md` | готов |

## 3. Readiness bundle

Минимальный состав latest bundle:

- `detmir-readiness-latest.json`;
- `detmir-readiness-act.md`;
- `detmir-readiness-act.html`;
- `sha256sums.txt`;
- `sha256sums.txt.sig`;
- `public-key.pem`;
- `detmir-readiness-status.json`;
- `detmir-readiness.prom`.

Критерии приемки:

- `detmir_readiness_ok 1`;
- `detmir_readiness_signature_verified 1`;
- `checksum_verified=true`;
- `signature.verified=true`;
- `signature.public_key_fingerprint_sha256` совпадает с публично
  зафиксированным fingerprint в `adk-rust/RUNBOOK.md`.

## 4. Portal readiness UI

В портале должен быть виден блок `Готовность системы`:

- статус `OK/WARN/FAIL`;
- дата генерации readiness bundle;
- статус подписи;
- статус checksum;
- короткий fingerprint публичного ключа;
- кнопка `Проверить bundle`.

Проверяемые API:

```text
GET /api/readiness/bundle
GET /api/readiness/verify
```

Через внешний gateway эти endpoints должны оставаться под действующей схемой
аутентификации портала.

## 5. Prometheus/Grafana alerts

Alert rules:

```promql
detmir_readiness_ok == 0
detmir_readiness_signature_verified == 0
```

Поставочный файл:

```text
aw-server/detmir-readiness-alerts.yml
```

Цель: оператор должен увидеть не только падение health-check, но и нарушение
доказуемости readiness bundle.

## 6. Проверки перед публикацией

```bash
node --check adk-rust/crates/detmir-portal/src/static/app.js
cargo fmt --manifest-path adk-rust/Cargo.toml --all -- --check
CARGO_TARGET_DIR=/tmp/detmir-release-target \
  cargo test --manifest-path adk-rust/Cargo.toml -p detmir-readiness -p detmir-portal
CARGO_TARGET_DIR=/tmp/detmir-release-target \
  cargo clippy --manifest-path adk-rust/Cargo.toml -p detmir-readiness -p detmir-portal --all-targets -- -D warnings
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml --syntax-check
ansible-playbook -i ansible/inventory.ini ansible/deploy_detmir_portal.yml --syntax-check
git diff --check
```

Public hygiene gate:

```bash
PRIVATE_MARKERS_REGEX='<PRIVATE_HOSTNAME>|<PRIVATE_NETWORK>|<PRIVATE_DOMAIN>|<LOCAL_OPERATOR_HOME>|<ROOT_PRIVATE_PATH>'
git grep -n -E "$PRIVATE_MARKERS_REGEX" -- \
  ':!docs/RELEASE_AUDIT_2026-06.md' ':!*.zip' ':!*.tar.gz' ':!adk-rust/target/**' || true
git ls-files | grep -E '(^|/)secrets(/|$)|\.env$|inventory\.ini$' || true
```

## 7. Ограничения

- Screenshots для публичной поставки должны быть обезличены и сняты на mock
  data или demo contour.
- Live inventory, live domains, private IPs, case IDs, forensic paths,
  screenshots с реальными пользователями и runtime evidence не входят в Git.
- Telegram runtime остается Python.
- pfSense/network layer не входит в этот release-readiness этап.

## 8. Следующий уровень

Для `release-readiness-v0.2` нужны:

- машинный CycloneDX/SPDX SBOM как release asset;
- подписанный Git tag;
- release asset checksum verification в CI;
- smoke screenshots как CI artifact;
- отдельный customer pilot acceptance акт.
