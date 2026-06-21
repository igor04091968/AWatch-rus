# Wiki and documentation policy

Статус: registry-readiness document. Документ фиксирует, какие источники
документации считаются доказательными для подготовки к реестру российского ПО.

## Политика документации

- GitHub Wiki repository
  `https://github.com/igor04091968/AWatch-rus.wiki.git` не обнаружен /
  не используется как обязательный источник.
- Gitea встроенная Wiki может использоваться как навигационная стартовая
  страница.
- Основной доказательный документальный пакет хранится в `docs/registry/`.
- `README.md` должен ссылаться на `docs/registry/`.
- Wiki не должна содержать единственные экземпляры критичных документов.
- При необходимости первая страница Wiki должна ссылаться на `README.md` и
  `docs/registry/`.

## Рекомендуемый текст для Gitea Wiki Home

```markdown
# AWatch-rus

Основная документация проекта находится в репозитории:

* README.md
* docs/
* docs/registry/

## Реестр российского ПО

Материалы подготовки к реестру российского ПО находятся в:

* docs/registry/REGISTER_RU_SOFTWARE_READINESS_RU.md
* docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md
* docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md
* docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md

## Основной российский Git-контур

https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus

GitHub используется как публичное зеркало.
```

## Ограничения

Wiki не является единственным источником доказательных документов и не заменяет
tracked files в репозитории. Для registry-readiness ссылаться на committed
документы из `docs/registry/`.
