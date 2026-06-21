# AWatch-rus: readiness для реестра российского ПО

Статус: подготовительный документ. Документ фиксирует текущее состояние
registry-readiness пакета, не подтверждает готовность подачи и не подтверждает
юридический результат рассмотрения AWatch-rus для реестра российского ПО.

## Назначение

Документ нужен для внутренней подготовки пакета сведений о продукте,
исходном коде, инфраструктуре хранения, выпуске, резервном копировании и
оставшихся пробелах перед финальной юридической проверкой.

Новые сведения по российскому Git-контуру требуют финального подтверждения
правообладателем и внесения в официальный пакет документов перед подачей.

## Текущее состояние инфраструктуры

- Российский Git-контур развернут как текущий контур для registry-readiness.
- Self-hosted Gitea развернута на REG.RU VPS / cloud server.
- Основной целевой URL:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- Gitea содержит дубликат/зеркало GitHub-репозитория AWatch-rus.
- GitHub используется как публичное зеркало и public validation surface.
- Gitea operator account: `igor`; пароль/токены хранятся вне репозитория.
- Встроенная Gitea Wiki может использоваться только как навигационная
  страница.
- Доказательная документация должна храниться в `docs/registry/`.
- Backup-контур начат, но `restore_tested=false` до проверки восстановления
  на отдельном сервере.
- Public CI, coverage baseline and security scanning added on GitHub for
  transparency.
- GitHub Actions is public mirror validation only and is not the primary
  registry build contour.
- Russian build-runner remains required for registry release candidate and
  release evidence.

## Текущее состояние

Done / partially done:

- REG.RU VPS создан.
- Gitea установлена.
- HTTPS включен.
- Организация `awatch-rus` создана.
- Репозиторий AWatch-rus мигрирован.
- `docs/registry/` используется как основной документальный пакет.
- Backup через `gitea dump` начат.
- Развернут текущий российский Git-контур для registry-readiness.
- Развернута self-hosted Gitea на REG.RU VPS / cloud server.
- Репозиторий AWatch-rus создан в Gitea как дубликат/зеркало GitHub:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- HTTPS для Gitea включен через Nginx reverse proxy.
- Начат backup-контур Gitea на базе `gitea dump`, ZIP-архивов,
  SHA256 checksum и целевого systemd timer.
- GitHub описывается как public mirror only, не как target primary
  source/build/release contour для registry-readiness.

## Что еще требуется

- Финальное подтверждение правообладателя.
- Российский build-runner или документированная российская сборочная среда.
- Хранение release artifacts на территории РФ или отдельное подтверждение
  выбранной схемы хранения.
- Протестированная restore-процедура Gitea на отдельном сервере.
- Документированная access control policy для Gitea.
- Offsite backup в РФ.
- Финальная юридическая проверка registry package.

## Новый статус registry-readiness

- Russian Git contour: partially done / done.
- Gitea backup: partially done.
- Russian build-runner: planned.
- Release artifacts storage in RF: planned.
- Release evidence automation: partially done after this task.
- Public CI transparency: added.
- Coverage baseline: added, threshold not enforced yet.
- Security scanning: added.
- Restore test: required.
- Legal rightsholder confirmation: required.

## Ограничения формулировок

AWatch-rus не заявляется как сертифицированное средство защиты информации,
сертифицированная DLP, SIEM или замена штатным средствам ИБ. В текущем пакете
не фиксируется наличие сертификации ФСТЭК или ФСБ.

REG.RU/Gitea контур сам по себе не является юридически достаточным
доказательством для реестра. Он рассматривается как инфраструктурная часть
registry-readiness и должен быть подтвержден в официальном пакете документов.
