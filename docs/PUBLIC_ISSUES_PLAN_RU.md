# AWatch-rus: план публичных GitHub issues

Дата: 2026-06-22

Статус: public governance backlog plan; issue templates are
`ready_to_create`.

Этот документ перечисляет публичные GitHub issues, которые нужно завести
вручную или через opt-in `gh` script. Он не утверждает, что задачи уже созданы
или выполнены.

Цель: повысить visibility development process после настройки российского
Gitea-контура, backup, public CI, coverage, security scanning и status freeze.

Подготовленный пакет:

- issue templates: `docs/public-issues/`;
- machine manifest:
  `docs/public-issues/public-issues-manifest.json`;
- creation runbook: `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`;
- dry-run check: `scripts/prepare_public_issues.sh`;
- opt-in creation script: `scripts/create_public_issues_from_manifest.sh`.

Реальные GitHub issue URLs остаются pending, пока они не записаны в manifest.

## Issues to create manually

| Title | Labels | Short goal | Acceptance criteria | Status |
| --- | --- | --- | --- | --- |
| `[registry] Perform Gitea backup restore test` | `registry`, `ops`, `evidence` | Prove restore procedure on a separate host and keep `restore_tested=false` until evidence exists. | Restore log, checksum verification, post-restore checks and rollback notes are attached or linked. | ready_to_create |
| `[registry] Prepare temporary Russian build-runner awatch-build-01` | `registry`, `build-runner`, `ops` | Provision temporary or permanent Russian build-runner for registry release evidence. | Host provisioning notes, toolchain list, Gitea access method and required checks plan are documented. | ready_to_create |
| `[release] Produce first release evidence package` | `release`, `registry`, `evidence` | Run release evidence scripts on `awatch-build-01` and collect artifacts/logs/checksums. | Release evidence manifest, logs, checksums and artifact storage path are documented. | ready_to_create |
| `[legal] Prepare rightsholder evidence package` | `legal`, `registry`, `docs` | Prepare rightsholder and legal evidence for future registry submission. | Rightsholder evidence checklist, ownership notes and legal review TODOs are documented. | ready_to_create |
| `[qa] Define coverage threshold policy` | `qa`, `coverage`, `policy` | Define threshold only after stable coverage baseline review. | Coverage baseline reviewed and initial threshold policy proposed without blocking current baseline workflow. | ready_to_create |
| `[security] Prepare external security/code review checklist` | `security`, `review`, `governance` | Establish visible peer review and external security review checklist. | Checklist references `docs/REVIEW_CHECKLIST_RU.md` and defines public review evidence expectations. | ready_to_create |
| `[compat] Test Russian OS compatibility matrix` | `compat`, `qa`, `registry` | Validate supported Russian OS matrix and document evidence. | Matrix lists target OS versions, test status and gaps without unsupported compatibility claims. | ready_to_create |
| `[ops] Validate release artifacts storage in RF` | `ops`, `release`, `registry` | Confirm release artifact storage location and retention in the Russian contour. | Storage path, retention, access model and checksum verification procedure are documented. | ready_to_create |
| `[docs] Refresh public demo pack and screenshots` | `docs`, `demo`, `public` | Update public demo pack, screenshots and non-sensitive demo evidence. | Demo materials contain no secrets, PII, real employee data or customer infrastructure identifiers. | ready_to_create |
| `[pilot] Prepare Pilot Acceptance Checklist v2` | `pilot`, `qa`, `docs` | Update pilot acceptance checklist after residual risk register and public issue plan. | Checklist references residual risks, smoke checks and acceptance evidence needed for pilot stage. | ready_to_create |
| `[governance] Enable PR-based review workflow` | `governance`, `review`, `process` | Move visible changes through pull requests where practical. | First public PR review record exists or a documented dry-run PR demonstrates the process. | ready_to_create |
| `[governance] Add branch protection policy` | `governance`, `github`, `policy` | Configure GitHub branch protection after maintainer review of the advisory policy. | Branch protection settings are documented with screenshots or notes, or blockers are recorded. | ready_to_create |

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
