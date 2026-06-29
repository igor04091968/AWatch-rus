# AWatch-rus: статус проекта на 2026-06-23

Документ фиксирует текущий baseline после настройки российского Gitea-контура,
backup, registry-readiness документации, плана российского build-runner и
публичного слоя GitHub Actions visibility.

Это статус подготовки и инженерной прозрачности. Он не является юридическим
заключением, подтверждением регистрации в реестре российского ПО или
доказательством сертификации.

## Текущий baseline

- Baseline commit: `4970d31 chore(public): add CI coverage security and OSS process visibility`.
- Public validation after hardening commit:
  `4f90aba chore(security): harden public secret scan and document policy`.
- Primary Russian Git:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- GitHub:
  `public mirror / public validation only`.
- Public CI status: passed.
- Public coverage workflow: passed.
- Public security workflow: passed.
- Secret scan: hardened and passed.
- GitHub Actions role: public mirror validation only.
- Gitea operator account: `igor`; пароль/токены не входят в tracked files.
- Основной доказательный пакет для registry-readiness:
  `docs/registry/`.
- Public validation passed status does not constitute registry release evidence.
- Registry release evidence still requires the Russian build-runner contour.
- Russian build-runner still required for registry release evidence.
- Остаточные риски:
  `docs/RESIDUAL_RISKS_RU.md`.
- План публичных GitHub issues:
  `docs/PUBLIC_ISSUES_PLAN_RU.md`.
- Пакет шаблонов публичных GitHub issues подготовлен:
  `docs/public-issues/`.
- Manifest публичных issues:
  `docs/public-issues/public-issues-manifest.json`.
- Public issue package: ready.
- Public issues: created and linked in manifest.
- Public development visibility: improved through created roadmap/governance
  issues.
- Community adoption remains low; no artificial adoption claim is made.
- Runbook создания публичных issues:
  `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`.
- Review checklist:
  `docs/REVIEW_CHECKLIST_RU.md`.
- Advisory branch protection policy:
  `docs/BRANCH_PROTECTION_POLICY_RU.md`.
- PR-based workflow documentation: ready
  (`docs/PR_REVIEW_WORKFLOW_RU.md`).
- PR review evidence package: ready
  (`docs/PR_REVIEW_EVIDENCE_RU.md`).
- Branch protection evidence package: ready
  (`docs/BRANCH_PROTECTION_EVIDENCE_RU.md`).
- Branch protection ruleset: `verified_active_ruleset`.
- Branch protection target branch: `main`.
- Branch protection required checks: `Coverage baseline`, `security`,
  `rust-checks`, `docs-registry-checks`, `smoke-checks`.
- First protected PR workflow: PR #50 opened; required checks passed;
  review/merge still `pending_review_required`.
- First reviewed PR evidence: pending.
- DetMir portal live baseline on 2026-06-25:
  `docs/DETMIR_CURRENT_STATE_RU.md`.
- DetMir portal deployed binary:
  `653b22b0fbf29a22f7de42ade7b689490b1de16fa07e785e4e0efd3078e7a3bc`.
- DetMir portal cold-start UI hang: mitigated. During cold/prewarm state the UI
  now shows `STALE / Первичный срез прогревается`, not endless loading.
- DetMir DLP hot-path boundary: phase 1 deployed on the portal service with
  `DETMIR_PORTAL_DLP_MODULE_ENABLED=false`.
- DetMir optional DLP runtime controls: implemented in code/docs through
  `AW_DLP_ENABLED`, `DETMIR_DLP_ENABLED`,
  `scripts/detmir_dlp_runtime_control.sh` and
  `docs/DLP_OPTIONAL_RUNTIME_RU.md`.
- DetMir optional DLP runtime live state: disabled on 2026-06-25 to reduce
  InfluxDB/Grafana/ClickHouse/AW server load. Evidence:
  `dlp-health-check=dlp:mode disabled`, `detmir-dlp=dlp:mode disabled`,
  active/enabled DLP units `0/0`, history snapshots under
  `/var/lib/activitywatch/health/dlp-runtime-history/`.
- DetMir DLP buckets in manual full check: `SKIPPED` under
  `AW_DLP_ENABLED=false`, not reported as dead.
- DetMir RDP collector freshness after 2026-06-29 restore: physical RDP target
  is `192.168.100.19`, stable AW logical host id remains `SHARKON2025`.
  Buckets are fresh/inactive as expected, collector guard quarantine was reset,
  and `check-aw-full` was updated to use env-driven RDP target and AFK
  `metadata.end` freshness. Admin laptop route to `192.168.100.19` goes through
  DetMir OpenVPN gateway `10.0.13.1`; WinRM `5985` and RDP `3389` are reachable
  from the admin laptop.
- Low-cost containment pack: first safe control-plane layer implemented as
  Rust `containment-engine`, example policy/finding fixtures, Ansible/env
  disabled-by-default configuration, and operator/policy runbooks. Current
  engine is decision/shadow only and does not mutate firewall, pfSense, AD,
  VLAN or routes.
- Security Finding Inbox: ClickHouse schema, DetMir Portal view/API,
  Hayabusa/Velociraptor ingest adapters and separate
  `security-finding-inbox executor` are implemented. Portal records workflow
  events only; approved `apply_requested` can be processed by the fail-closed
  executor through `containment-engine` `decide -> plan -> apply -> verify`
  with rollback on apply failure.
- Security Finding Inbox live schema: applied on ClickHouse 2026-06-29
  (`security_findings`, `security_finding_workflow_events`,
  `security_finding_inbox`).
- DetMir Loki log contour: intentionally disabled by operator to reduce
  resource usage. Proxmox LXC `202 loki-logs` is stopped, active config has
  `onboot: 0`, and smoke checks skip Loki by default unless
  `AW_SMOKE_LOKI_ENABLED=1` is set.
- DetMir restore baseline 2026-06-29:
  `docs/DETMIR_RESTORE_BASELINE_2026-06-29_RU.md`.
- DetMir API smoke after phase 1: `/healthz` and `/readyz` OK;
  `/api/reports` returned `cache_status=warming` with
  `modules.dlp.enabled=false`; `/api/operator` returned bounded
  `cache_status=warming` / `summary.severity=STALE` without waiting for the
  full cold snapshot.
- DetMir remaining heavy path risk: full report/snapshot prewarm can still be
  CPU/IO expensive; deeper optimization remains pending.
- DetMir VPN access rule: do not identify the contour by `tun0`/`tun1`; verify
  by NetworkManager profile, `10.0.13.*` address, route via `10.0.13.1` and live
  reachability.

## Что готово

- Развернут целевой российский Git-контур на REG.RU VPS / cloud server.
- Развернута self-hosted Gitea.
- Настроен HTTPS-доступ к Gitea через домен
  `https://git.iri1968.dpdns.org`.
- Создана организация `awatch-rus` и репозиторий
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- Gitea-репозиторий создан как дубликат/зеркало GitHub-репозитория AWatch-rus.
- Текущая локальная рабочая копия на этой машине пока имеет `origin=GitHub`;
  для прямой синхронизации с Gitea добавить `ru-origin` по
  `docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md`.
- GitHub зафиксирован как публичное зеркало и публичная поверхность проверки,
  а не как основной registry source/build/release contour.
- Начат backup-контур Gitea через `gitea dump`.
- Зафиксированы SHA256 checksums для backup evidence.
- Описан backup timer `awatch-gitea-backup.timer`.
- Подготовлен registry-readiness пакет в `docs/registry/`.
- Добавлены runbook и скрипты release evidence:
  `scripts/build_release_evidence.sh` и
  `scripts/check_release_evidence.sh`.
- Добавлены публичные GitHub Actions workflow для CI, coverage baseline и
  security scanning.
- Первый публичный GitHub Actions validation после hardening secret scan прошел
  по контурам `CI`, `Coverage` и `Security`.
- Добавлены `SECURITY.md`, `CONTRIBUTING.md`, `ROADMAP.md`, issue templates и
  pull request template.
- Добавлен `.github/CODEOWNERS` for review routing.
- Добавлен review checklist:
  `docs/REVIEW_CHECKLIST_RU.md`.
- PR review process documented in PR template and review checklist.
- Branch protection policy documented as advisory:
  `docs/BRANCH_PROTECTION_POLICY_RU.md`.
- Branch protection evidence template prepared:
  `docs/BRANCH_PROTECTION_EVIDENCE_RU.md`.
- GitHub ruleset / branch protection for `main` verified active by maintainer:
  ruleset `main`, target branch `main`, empty bypass list, required PR with
  one approval, stale approval dismissal, Code Owners review, required status
  checks and force-push blocking.
- PR-based review workflow documented:
  `docs/PR_REVIEW_WORKFLOW_RU.md`.
- PR review evidence template prepared:
  `docs/PR_REVIEW_EVIDENCE_RU.md`.
- First protected PR workflow evidence recorded for PR #50:
  required checks passed; review requirement is still pending; merge status is
  open; no admin bypass recorded.
- Зафиксирован residual risk register:
  `docs/RESIDUAL_RISKS_RU.md`.
- Подготовлен план публичных issues для ручного заведения:
  `docs/PUBLIC_ISSUES_PLAN_RU.md`.
- Подготовлен пакет issue templates:
  `docs/public-issues/`.
- Подготовлен runbook ручного/opt-in создания issues:
  `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`.
- Созданы 12 публичных roadmap/governance GitHub issues, ссылки записаны в
  `docs/public-issues/public-issues-manifest.json`.

## Planned / pending

- Тестовое восстановление Gitea backup на отдельном сервере.
- Provisioning российского build-runner `awatch-build-01`.
- Первый реальный release evidence build на российском build-runner.
- Хранилище release artifacts в российском контуре.
- Юридическое подтверждение правообладателя.
- Финальная юридическая проверка пакета документов перед подачей.
- Проверка совместимости с российскими ОС.
- Visible external code review is still pending.
- First reviewed PR evidence remains pending until a reviewed public PR is
  merged and evidence is recorded.
- External peer review remains pending.
- Community adoption remains low until external contributors, public reviews
  and sustained third-party activity appear.
- DetMir DLP runtime disable is complete for the current live contour; deeper
  long-term DLP product modularization and retention/cleanup policy remain
  separate future work.
- DetMir RDP collector/session recovery after 2026-06-29 restore is verified by
  live smoke: `check-aw-full` reports `FRESH=8 STALE=0 DEAD=0`.

## Честные ограничения

- Не заявляется наличие сертификации ФСТЭК или ФСБ.
- Не заявляется замена SIEM или DLP.
- No claim: ML/LLM-based detection.
- No claim: automatic remediation.
- Подача и регистрация в реестре российского ПО не завершены.
- REG.RU/Gitea контур и registry-readiness документы требуют подтверждения
  правообладателем и включения в официальный пакет документов.

## Следующие рекомендуемые этапы

1. Выполнить тестовое восстановление Gitea backup на отдельном сервере.
2. Подготовить временный или постоянный российский build-runner
   `awatch-build-01`.
3. Сформировать первый release evidence package на российском build-runner.
4. Подготовить юридический пакет правообладателя для финальной проверки.

## Связанные документы

- `docs/registry/REGISTER_RU_SOFTWARE_READINESS_RU.md`
- `docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md`
- `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`
- `docs/registry/RU_BUILD_RUNNER_READINESS_RU.md`
- `docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md`
- `docs/registry/RELEASE_ARTIFACTS_STORAGE_RU.md`
- `docs/QUALITY_STATUS_RU.md`
- `docs/REVIEW_CHECKLIST_RU.md`
- `docs/PR_REVIEW_WORKFLOW_RU.md`
- `docs/PR_REVIEW_EVIDENCE_RU.md`
- `docs/RESIDUAL_RISKS_RU.md`
- `docs/PUBLIC_ISSUES_PLAN_RU.md`
- `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`
- `docs/public-issues/public-issues-manifest.json`
- `docs/BRANCH_PROTECTION_POLICY_RU.md`
- `docs/BRANCH_PROTECTION_EVIDENCE_RU.md`
- `docs/DETMIR_CURRENT_STATE_RU.md`
