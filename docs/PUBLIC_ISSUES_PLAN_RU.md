# AWatch-rus: план публичных GitHub issues

Дата: 2026-06-23

Статус: public governance backlog plan; issue templates are prepared and 12
public GitHub issues are recorded in the manifest.

Этот документ перечисляет публичные GitHub issues, созданные из подготовленных
templates. Он не утверждает, что сами задачи уже выполнены.

Цель: повысить visibility development process после настройки российского
Gitea-контура, backup, public CI, coverage, security scanning и status freeze.

Подготовленный пакет:

- issue templates: `docs/public-issues/`;
- machine manifest:
  `docs/public-issues/public-issues-manifest.json`;
- creation runbook: `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`;
- dry-run check: `scripts/prepare_public_issues.sh`;
- opt-in creation script: `scripts/create_public_issues_from_manifest.sh`.

Реальные GitHub issue URLs записаны в manifest. GitHub issues являются public
roadmap/development visibility, а не registry release evidence. GitHub остается
public mirror validation only; primary registry contour остается Russian Gitea
и planned Russian build-runner.

## Созданные публичные issues

Создано 12 из 12 planned issues:

- `[registry] Perform Gitea backup restore test`:
  https://github.com/igor04091968/AWatch-rus/issues/38
- `[registry] Prepare temporary Russian build-runner awatch-build-01`:
  https://github.com/igor04091968/AWatch-rus/issues/39
- `[release] Produce first release evidence package`:
  https://github.com/igor04091968/AWatch-rus/issues/40
- `[legal] Prepare rightsholder evidence package`:
  https://github.com/igor04091968/AWatch-rus/issues/41
- `[qa] Define coverage threshold policy`:
  https://github.com/igor04091968/AWatch-rus/issues/42
- `[security] Prepare external security/code review checklist`:
  https://github.com/igor04091968/AWatch-rus/issues/43
- `[compat] Test Russian OS compatibility matrix`:
  https://github.com/igor04091968/AWatch-rus/issues/44
- `[ops] Validate release artifacts storage in RF`:
  https://github.com/igor04091968/AWatch-rus/issues/45
- `[docs] Refresh public demo pack and screenshots`:
  https://github.com/igor04091968/AWatch-rus/issues/46
- `[pilot] Prepare Pilot Acceptance Checklist v2`:
  https://github.com/igor04091968/AWatch-rus/issues/47
- `[governance] Enable PR-based review workflow`:
  https://github.com/igor04091968/AWatch-rus/issues/48
- `[governance] Add branch protection policy`:
  https://github.com/igor04091968/AWatch-rus/issues/49

These URLs improve public roadmap/development visibility only. They do not
prove restore completion, Russian build-runner readiness, release evidence
production, legal readiness, external peer review, branch protection enablement
or community adoption.

## Issues created from templates

| Title | Labels | Short goal | Acceptance criteria | Status |
| --- | --- | --- | --- | --- |
| `[registry] Perform Gitea backup restore test` | `registry`, `ops`, `evidence` | Prove restore procedure on a separate host and keep `restore_tested=false` until evidence exists. | Restore log, checksum verification, post-restore checks and rollback notes are attached or linked. | created |
| `[registry] Prepare temporary Russian build-runner awatch-build-01` | `registry`, `build-runner`, `ops` | Provision temporary or permanent Russian build-runner for registry release evidence. | Host provisioning notes, toolchain list, Gitea access method and required checks plan are documented. | created |
| `[release] Produce first release evidence package` | `release`, `registry`, `evidence` | Run release evidence scripts on `awatch-build-01` and collect artifacts/logs/checksums. | Release evidence manifest, logs, checksums and artifact storage path are documented. | created |
| `[legal] Prepare rightsholder evidence package` | `legal`, `registry`, `docs` | Prepare rightsholder and legal evidence for future registry submission. | Rightsholder evidence checklist, ownership notes and legal review TODOs are documented. | created |
| `[qa] Define coverage threshold policy` | `qa`, `coverage`, `policy` | Define threshold only after stable coverage baseline review. | Coverage baseline reviewed and initial threshold policy proposed without blocking current baseline workflow. | created |
| `[security] Prepare external security/code review checklist` | `security`, `review`, `governance` | Establish visible peer review and external security review checklist. | Checklist references `docs/REVIEW_CHECKLIST_RU.md` and defines public review evidence expectations. | created |
| `[compat] Test Russian OS compatibility matrix` | `compat`, `qa`, `registry` | Validate supported Russian OS matrix and document evidence. | Matrix lists target OS versions, test status and gaps without unsupported compatibility claims. | created |
| `[ops] Validate release artifacts storage in RF` | `ops`, `release`, `registry` | Confirm release artifact storage location and retention in the Russian contour. | Storage path, retention, access model and checksum verification procedure are documented. | created |
| `[docs] Refresh public demo pack and screenshots` | `docs`, `demo`, `public` | Update public demo pack, screenshots and non-sensitive demo evidence. | Demo materials contain no secrets, PII, real employee data or customer infrastructure identifiers. | created |
| `[pilot] Prepare Pilot Acceptance Checklist v2` | `pilot`, `qa`, `docs` | Update pilot acceptance checklist after residual risk register and public issue plan. | Checklist references residual risks, smoke checks and acceptance evidence needed for pilot stage. | created |
| `[governance] Enable PR-based review workflow` | `governance`, `review`, `process` | Move visible changes through pull requests where practical. | First public PR review record exists or a documented dry-run PR demonstrates the process. | created |
| `[governance] Add branch protection policy` | `governance`, `github`, `policy` | Configure GitHub branch protection after maintainer review of the advisory policy. | Branch protection settings are documented with screenshots or notes, or blockers are recorded. | created |

## Guardrails

- Do not mark restore test as completed until restore evidence exists.
- Do not mark `awatch-build-01` as ready until provisioning evidence exists.
- Do not mark release evidence as produced until artifacts and checksums exist.
- Do not claim completed registry submission.
- Do not claim fake community adoption.
- Do not position GitHub Actions as the primary registry build contour.
- Do not claim external peer review is active until public reviewed PRs exist.
- Do not claim branch protection is enabled until repository settings are
  verified.
