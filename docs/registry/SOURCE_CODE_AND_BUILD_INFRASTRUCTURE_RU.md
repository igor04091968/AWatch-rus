# Исходный код и инфраструктура сборки AWatch-rus

Статус: registry-readiness document. Документ описывает текущую целевую
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
| Role | target primary Russian Git contour for registry-readiness |
| GitHub role | public mirror only / external public repository |
| HTTPS | enabled |
| Reverse proxy | Nginx reverse proxy |
| Gitea local HTTP endpoint | `127.0.0.1:3000` |
| External port policy | external `3000/tcp` should not be exposed after HTTPS validation |

GitHub = public mirror only. GitHub не должен описываться как primary
registry source/build/release contour для registry-readiness. Он может
использоваться как публичное зеркало и внешняя площадка, но целевой российский
контур хранения исходного кода для подготовки к реестру зафиксирован в Gitea.

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
