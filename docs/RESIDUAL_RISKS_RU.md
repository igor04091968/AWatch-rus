# AWatch-rus: остаточные риски после public validation и российского Git-контура

Дата: 2026-06-22

Статус: governance / registry-readiness residual risk register.

Документ фиксирует оставшиеся риски после настройки российского Gitea-контура,
backup, public GitHub Actions validation, coverage workflow, security scanning
и status freeze.

Это не заявление о завершенной регистрации в реестре российского ПО и не
release evidence. GitHub Actions остается public mirror validation only.
Primary registry-readiness contour остается: Russian Gitea, planned Russian
build-runner и будущий release evidence build на российском контуре.

## Архитектурный вывод

Текущий pilot/readiness stage не блокируется перечисленными рисками, потому что
ядро инженерной прозрачности уже зафиксировано:

- source contour documented: self-hosted Russian Gitea;
- public mirror validation passed: CI, Coverage, Security;
- secret scan hardened and passed;
- backup process documented with SHA256 verification and daily timer;
- registry-readiness docs and release evidence scripts exist;
- forbidden positioning claims are explicitly excluded.

Оставшиеся риски относятся к governance, disaster recovery proof, public process
visibility, release evidence contour and legal package. Они требуют дальнейших
действий до registry release evidence / GA, но не отменяют pilot/readiness
статус.

## 1. Один основной разработчик

- Текущий статус: риск открыт; основная инженерная экспертиза сосредоточена у
  одного maintainer.
- Влияние: задержка развития, поддержки и incident response при недоступности
  maintainer; повышенная зависимость от личной экспертизы.
- Почему не блокирует pilot/readiness stage: архитектура, deployment docs,
  runbooks, registry docs and public checks already create a transferable
  baseline for pilot validation.
- Как риск будет снижаться: second maintainer onboarding, documented code
  ownership, mandatory PR review for release branches, knowledge transfer
  sessions.
- Уже снижающие evidence/documents/CI: README, `docs/PROJECT_STATUS_RU.md`,
  `docs/QUALITY_STATUS_RU.md`, `docs/registry/`, GitHub Actions CI/Coverage/
  Security, issue templates and PR template.
- Следующий action: завести публичную задачу
  `[security] Prepare external security/code review checklist`.

## 2. Нет внешнего visible peer review / публично видимого peer review

- Текущий статус: риск открыт; PR template and issue templates exist, but
  public peer review history is still limited and external review is pending.
- Влияние: внешним аудиторам сложнее оценить review discipline and change
  control maturity.
- Почему не блокирует pilot/readiness stage: current changes are protected by
  reproducible checks, public workflows and documented conservative positioning.
- Как риск будет снижаться: review checklist, CODEOWNERS routing, first public
  PR reviews, explicit release branch review policy and advisory branch
  protection.
- Уже снижающие evidence/documents/CI: `.github/pull_request_template.md`,
  `.github/CODEOWNERS`, `.github/ISSUE_TEMPLATE/`,
  `docs/REVIEW_CHECKLIST_RU.md`, `docs/BRANCH_PROTECTION_POLICY_RU.md`,
  `CONTRIBUTING.md`, `SECURITY.md`, public CI, public security workflow.
- Следующий action: завести публичную задачу
  `[governance] Enable PR-based review workflow`.

## 3. Низкая публичная активность issue tracker

- Текущий статус: риск открыт частично; issue templates, public roadmap and
  issue creation package exist, but real GitHub issues are still pending until
  URLs are recorded.
- Влияние: низкая внешняя visibility development process; сложнее показать
  плановое управление backlog and governance.
- Почему не блокирует pilot/readiness stage: templates, roadmap and status docs
  already define expected process; missing public tasks are a visibility gap,
  not a runtime readiness gap.
- Как риск будет снижаться: manually create public issues for registry, QA,
  security, compatibility, ops and pilot follow-up work using
  `docs/public-issues/` and `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`.
- Уже снижающие evidence/documents/CI: `ROADMAP.md`, issue templates,
  `docs/PROJECT_STATUS_RU.md`, `docs/PUBLIC_ISSUES_PLAN_RU.md`,
  `docs/public-issues/public-issues-manifest.json`,
  `scripts/prepare_public_issues.sh`.
- Следующий action: создать реальные GitHub issues и записать URLs в
  `docs/public-issues/public-issues-manifest.json`.

## 4. Низкая community adoption

- Текущий статус: риск открыт; stars/forks remain low and the project still
  looks like early-stage / pilot-stage OSS.
- Влияние: нет широкого external validation and "many eyes" effect; меньше
  внешних сигналов доверия.
- Почему не блокирует pilot/readiness stage: это не технический blocker.
  Specialized enterprise/security OSS normally grows through pilots,
  documentation, demos, case studies and references.
- Как риск будет снижаться: public demo pack, updated screenshots, pilot
  materials, external links, publications and first customer pilots.
- Уже снижающие evidence/documents/CI: README, demo docs, pilot docs,
  screenshots, public workflows, registry docs.
- Следующий action: завести публичную задачу
  `[docs] Refresh public demo pack and screenshots`.

## 5. Gitea restore test еще не выполнен

- Текущий статус: риск открыт; backup works, SHA256 verification works and daily
  timer is documented, but `restore_tested` remains false.
- Влияние: disaster recovery capability is documented but not yet proven by a
  restore drill on a separate host.
- Почему не блокирует pilot/readiness stage: backup contour already exists and
  can support readiness documentation; release/registry evidence still requires
  restore proof later.
- Как риск будет снижаться: perform restore test on a separate server, record
  logs, checksum verification, post-restore checks and rollback notes.
- Уже снижающие evidence/documents/CI:
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`,
  `docs/registry/registry-evidence-manifest.json`,
  `scripts/registry_readiness_check.sh`.
- Следующий action: завести публичную задачу
  `[registry] Perform Gitea backup restore test`.

## 6. Российский build-runner пока planned

- Текущий статус: риск открыт; `awatch-build-01` is planned, not ready.
- Влияние: release evidence cannot yet be produced on the target Russian
  build-runner contour.
- Почему не блокирует pilot/readiness stage: public GitHub Actions provides
  mirror validation, while registry release evidence is explicitly deferred to
  the Russian build-runner.
- Как риск будет снижаться: provision temporary or permanent `awatch-build-01`,
  install toolchain, connect to Russian Gitea and run required checks.
- Уже снижающие evidence/documents/CI:
  `docs/registry/RU_BUILD_RUNNER_READINESS_RU.md`,
  `docs/registry/BUILD_RUNNER_SETUP_RUNBOOK_RU.md`, public CI/Coverage/
  Security as non-release validation.
- Следующий action: завести публичную задачу
  `[registry] Prepare temporary Russian build-runner awatch-build-01`.

## 7. Первый настоящий release evidence build pending

- Текущий статус: риск открыт; release evidence scripts exist, but the first
  real release evidence build on `awatch-build-01` has not been performed.
- Влияние: registry release evidence package is not yet available from the
  target build contour.
- Почему не блокирует pilot/readiness stage: pilot readiness can use current
  docs and public validation; registry release evidence is a later gate.
- Как риск будет снижаться: run release evidence scripts on the Russian
  build-runner, collect artifacts, checksums, cargo metadata/tree, logs and
  release manifest.
- Уже снижающие evidence/documents/CI:
  `scripts/build_release_evidence.sh`,
  `scripts/check_release_evidence.sh`,
  `docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md`,
  `docs/registry/RELEASE_EVIDENCE_MANIFEST_RU.md`.
- Следующий action: завести публичную задачу
  `[release] Produce first release evidence package`.

## 8. Юридический пакет правообладателя pending

- Текущий статус: риск открыт; technical readiness is strong, but rightsholder
  evidence package is not yet finalized.
- Влияние: registry submission cannot be treated as legally ready without
  ownership, rights and submission documentation.
- Почему не блокирует pilot/readiness stage: pilot/readiness is technical and
  operational; legal package is a separate submission track.
- Как риск будет снижаться: prepare rightsholder documents, ownership evidence,
  dependency review summary and legal review checklist.
- Уже снижающие evidence/documents/CI: registry docs, dependency statement,
  third-party license docs, conservative README positioning, public security
  and coverage validation.
- Следующий action: завести публичную задачу
  `[legal] Prepare rightsholder evidence package`.

## Review/governance evidence added

- CODEOWNERS exists for review routing and engineering ownership.
- PR review checklist exists in `docs/REVIEW_CHECKLIST_RU.md`.
- Advisory branch protection policy exists in
  `docs/BRANCH_PROTECTION_POLICY_RU.md`.
- Public PR template includes security, registry-claim, runtime/API/UI,
  smoke-test, rollback and evidence checklist items.
- Visible external code review remains pending until public reviewed PRs exist.

## Следующие публичные задачи

Полный список задач для ручного заведения в GitHub issue tracker:

- `docs/PUBLIC_ISSUES_PLAN_RU.md`.
- `docs/public-issues/`.
- `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`.
