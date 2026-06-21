# Russian build-runner readiness

Статус: registry-readiness plan. Документ описывает целевую архитектуру
российского build-runner для AWatch-rus и не утверждает, что build-runner уже
развернут или что release evidence уже production-ready.

## Назначение

Build-runner нужен для воспроизводимой сборки release candidate, выполнения
проверок, подготовки artifacts и формирования release evidence в российском
контуре. Он должен быть отделен от Gitea-сервера, чтобы тяжелые Rust workspace
build/test/check задачи не конкурировали с Git-хранилищем, HTTPS reverse proxy
и backup-процедурами.

GitHub Actions is public mirror validation only. GitHub Actions is not the
primary registry build contour and does not replace `awatch-build-01`.

## Целевая роль

| Параметр | Значение |
| --- | --- |
| Recommended hostname | `awatch-build-01` |
| Recommended provider | REG.RU VPS / cloud server или другой российский сервер |
| Target location | Russian Federation |
| Role | российский контур сборки и выпуска release artifacts |
| Source repository | `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus` |
| GitHub role | public mirror only |

## Почему не awatch-git-01

`awatch-git-01` должен оставаться Git-сервером: self-hosted Gitea,
repository, `docs/registry/`, HTTPS/Nginx и Gitea backup. Тяжелые сборки,
`cargo test --workspace`, `cargo clippy`, SBOM generation и packaging могут
создавать высокую нагрузку на CPU, RAM и диск. Поэтому registry-readiness
architecture разделяет:

- `awatch-git-01`: source repository and documentation contour;
- `awatch-build-01`: build/test/check/release evidence contour.

## Минимальная конфигурация

- 4 vCPU.
- 8 GB RAM.
- 100+ GB SSD/NVMe.
- Debian 12 или Ubuntu 24.04 LTS.

## Экономная конфигурация только для docs/check

- 2 vCPU.
- 4 GB RAM.
- 80 GB SSD.

Экономная конфигурация допустима только для documentation checks,
registry-readiness checks и легких smoke. Ее нельзя считать достаточной для
полного Rust workspace release build без отдельной проверки.

## Рекомендуемая конфигурация для Rust workspace

- 4 vCPU.
- 8-16 GB RAM.
- 120+ GB SSD/NVMe.
- Debian 12 или Ubuntu 24.04 LTS.

## Release artifacts storage

Release artifacts должны храниться в российском контуре. Целевые варианты
описаны в `docs/registry/RELEASE_ARTIFACTS_STORAGE_RU.md`. GitHub Releases
могут быть public mirror, но не primary release storage для registry-readiness.

## Evidence files

Build-runner должен формировать:

- source archive;
- binary artifacts archive;
- `SHA256SUMS`;
- `cargo-metadata.json`;
- `cargo-tree.txt`;
- SBOM CycloneDX, если инструмент доступен;
- SBOM SPDX, если инструмент доступен;
- logs for fmt/test/clippy/build/registry/smoke checks;
- release evidence manifest;
- release evidence report;
- version output, если применимо;
- health/readiness output, если применимо.

## Требует подтверждения правообладателем

- Фактический provider и location build-runner.
- Ответственный за build-runner.
- Политика доступа.
- Release artifacts storage в РФ.
- Offsite backup policy для release artifacts.
- Достаточность evidence для официального пакета документов.

## Текущий статус

- Russian Git contour: partially done / done.
- Gitea backup: partially done.
- Russian build-runner: planned.
- Release artifacts storage in RF: planned.
- Release evidence automation: partially done after this task.
- Public CI transparency: added.
- Coverage baseline: added.
- Security scanning: added.
- Restore test: required.
- Legal rightsholder confirmation: required.
