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
- Branch protection enablement is not claimed until repository settings are
  verified.
- Community adoption remains low until external contributors, public reviews
  and sustained third-party activity appear.

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
- `docs/RESIDUAL_RISKS_RU.md`
- `docs/PUBLIC_ISSUES_PLAN_RU.md`
- `docs/PUBLIC_ISSUES_CREATION_RUNBOOK_RU.md`
- `docs/public-issues/public-issues-manifest.json`
- `docs/BRANCH_PROTECTION_POLICY_RU.md`
