# AWatch-rus: план публичных GitHub issues

Дата: 2026-06-22

Статус: public governance backlog plan.

Этот документ перечисляет публичные GitHub issues, которые нужно завести
вручную. Он не утверждает, что задачи уже созданы или выполнены.

Цель: повысить visibility development process после настройки российского
Gitea-контура, backup, public CI, coverage, security scanning и status freeze.

## Issues to create manually

| Title | Purpose | Current status |
| --- | --- | --- |
| `[registry] Perform Gitea backup restore test` | Prove restore procedure on a separate host and keep `restore_tested=false` until evidence exists. | To create |
| `[registry] Prepare temporary Russian build-runner awatch-build-01` | Provision temporary or permanent Russian build-runner for registry release evidence. | To create |
| `[release] Produce first release evidence package` | Run release evidence scripts on `awatch-build-01` and collect artifacts/logs/checksums. | To create |
| `[legal] Prepare rightsholder evidence package` | Prepare rightsholder and legal evidence for future registry submission. | To create |
| `[qa] Define coverage threshold policy` | Define threshold only after stable coverage baseline review. | To create |
| `[security] Prepare external security/code review checklist` | Establish visible peer review and external security review checklist. | To create |
| `[compat] Test Russian OS compatibility matrix` | Validate supported Russian OS matrix and document evidence. | To create |
| `[ops] Validate release artifacts storage in RF` | Confirm release artifact storage location and retention in the Russian contour. | To create |
| `[docs] Refresh public demo pack and screenshots` | Update public demo pack, screenshots and non-sensitive demo evidence. | To create |
| `[pilot] Prepare Pilot Acceptance Checklist v2` | Update pilot acceptance checklist after residual risk register and public issue plan. | To create |

## Guardrails

- Do not mark restore test as completed until restore evidence exists.
- Do not mark `awatch-build-01` as ready until provisioning evidence exists.
- Do not mark release evidence as produced until artifacts and checksums exist.
- Do not claim completed registry submission.
- Do not claim fake community adoption.
- Do not position GitHub Actions as the primary registry build contour.

