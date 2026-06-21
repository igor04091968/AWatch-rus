# Исходный код и инфраструктура сборки AWatch-rus

Статус: registry-readiness document. Документ описывает текущую развернутую
инфраструктуру хранения исходного кода и связанные ограничения. Он не
фиксирует юридическую завершенность подачи в реестр российского ПО.

## Российский Git-контур

| Параметр | Значение |
| --- | --- |
| Provider | REG.RU VPS / cloud server |
| Platform | self-hosted Gitea |
| URL | `https://git.iri1968.dpdns.org` |
| Organization | `awatch-rus` |
| Repository | `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus` |
| Role | primary Russian Git contour and Gitea duplicate/mirror for registry-readiness |
| GitHub role | public mirror only / external public repository (public validation surface) |
| GitHub repository | `https://github.com/igor04091968/AWatch-rus` |
| Authorized operator account | `igor` |
| HTTPS | enabled |
| Reverse proxy | Nginx reverse proxy |
| Gitea local HTTP endpoint | `127.0.0.1:3000` |
| External port policy | external `3000/tcp` should not be exposed after HTTPS validation |

Gitea на `git.iri1968.dpdns.org` создана как российский дубликат/зеркало
публичного GitHub-репозитория AWatch-rus. GitHub = public mirror / public
validation surface only. GitHub не должен описываться как primary registry
source/build/release contour для registry-readiness. Он может использоваться
как публичное зеркало и внешняя площадка, но текущий российский контур хранения
исходного кода для подготовки к реестру зафиксирован в Gitea.

Учетная запись оператора Gitea: `igor`. Пароль, токены, private keys и другие
секреты не фиксируются в репозитории, evidence-пакете или Gitea Wiki; хранить
их нужно только в приватном хранилище учетных данных.

## Что изменилось на 2026-06-22

- Self-hosted Gitea на `https://git.iri1968.dpdns.org` используется как
  фактически созданный российский Git-дубликат/зеркало GitHub-репозитория.
- Организация `awatch-rus` и репозиторий `AWatch-rus` доступны в Gitea.
- HTTPS обслуживается через Nginx reverse proxy; локальный endpoint Gitea
  остается `127.0.0.1:3000`.
- Текущая локальная рабочая копия на этой машине все еще имеет `origin`,
  указывающий на GitHub. Для прямого push в Gitea добавить remote `ru-origin`
  по runbook `GIT_RU_MIRRORING_RUNBOOK_RU.md`.

## Gap

- Build-runner в РФ еще требуется.
- Release artifacts storage в РФ еще требуется.
- Backup restore test еще требуется.
- Legal ownership evidence еще требуется.

## Сборочный контур

Текущий документ фиксирует Git-контур хранения исходного кода. Российский
build-runner, release artifact storage в РФ и выпускные процедуры должны быть
описаны отдельно до финальной подачи.

До завершения этой части нельзя утверждать, что российский source/build/release
contour полностью закрыт. Допустимая формулировка: "развернут целевой
российский Git-контур для registry-readiness".

## Требования к подтверждению

Перед включением сведений в официальный пакет документов необходимо собрать:

- подтверждение владельца инфраструктуры и правообладателя;
- состояние Gitea service и Nginx;
- подтверждение HTTPS endpoint;
- подтверждение, что внешний `3000/tcp` не экспонируется после проверки HTTPS;
- список remotes рабочей копии;
- evidence по backup ZIP, SHA256 checksum и systemd timer.
