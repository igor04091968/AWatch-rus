# AWatch-rus: статус проекта на 2026-06-22

Документ фиксирует текущий baseline после настройки российского Gitea-контура,
backup, registry-readiness документации, плана российского build-runner и
публичного слоя GitHub Actions visibility.

Это статус подготовки и инженерной прозрачности. Он не является юридическим
заключением, подтверждением регистрации в реестре российского ПО или
доказательством сертификации.

## Текущий baseline

- Baseline commit: `4970d31 chore(public): add CI coverage security and OSS process visibility`.
- Primary Russian Git:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- GitHub:
  `public mirror / public validation only`.
- Gitea operator account: `igor`; пароль/токены не входят в tracked files.
- Основной доказательный пакет для registry-readiness:
  `docs/registry/`.

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
- Добавлены `SECURITY.md`, `CONTRIBUTING.md`, `ROADMAP.md`, issue templates и
  pull request template.

## Planned / pending

- Тестовое восстановление Gitea backup на отдельном сервере.
- Provisioning российского build-runner `awatch-build-01`.
- Первый реальный release evidence build на российском build-runner.
- Хранилище release artifacts в российском контуре.
- Юридическое подтверждение правообладателя.
- Финальная юридическая проверка пакета документов перед подачей.
- Проверка совместимости с российскими ОС.

## Честные ограничения

- Не заявляется наличие сертификации ФСТЭК или ФСБ.
- Не заявляется замена SIEM или DLP.
- No claim: ML/LLM-based detection.
- No claim: automatic remediation.
- Подача и регистрация в реестре российского ПО не завершены.
- REG.RU/Gitea контур и registry-readiness документы требуют подтверждения
  правообладателем и включения в официальный пакет документов.

## Следующие рекомендуемые этапы

1. Проверить первый публичный прогон GitHub Actions после push.
2. Выполнить тестовое восстановление Gitea backup на отдельном сервере.
3. Подготовить временный или постоянный российский build-runner
   `awatch-build-01`.
4. Сформировать первый release evidence package на российском build-runner.
5. Подготовить юридический пакет правообладателя для финальной проверки.

## Связанные документы

- `docs/registry/REGISTER_RU_SOFTWARE_READINESS_RU.md`
- `docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md`
- `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`
- `docs/registry/RU_BUILD_RUNNER_READINESS_RU.md`
- `docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md`
- `docs/registry/RELEASE_ARTIFACTS_STORAGE_RU.md`
- `docs/QUALITY_STATUS_RU.md`
