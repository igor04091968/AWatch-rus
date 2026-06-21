# Runbook: российский Git-контур и зеркалирование

Статус: operational registry-readiness runbook. Команды ниже описывают
целевую схему remotes для работы с российским self-hosted Gitea и публичным
GitHub mirror.

## Целевая схема remotes

Primary registry-readiness remote:

```text
ru-origin:
https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus.git
```

GitHub public mirror:

```text
github:
https://github.com/igor04091968/AWatch-rus.git
```

GitHub = public mirror only. GitHub не должен описываться как primary
registry source/build system или как целевой source/build/release contour
для registry-readiness.

## Базовые команды

Проверить текущие remotes:

```bash
git remote -v
```

Добавить российский remote:

```bash
git remote add ru-origin https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus.git
```

Запушить основную ветку:

```bash
git push ru-origin main
```

Запушить теги:

```bash
git push ru-origin --tags
```

Добавить GitHub mirror remote, если он еще не настроен:

```bash
git remote add github https://github.com/igor04091968/AWatch-rus.git
```

Обновить GitHub mirror:

```bash
git push github main
git push github --tags
```

## Операционные предупреждения

- Реальные remote names сначала проверить через `git remote -v`.
- При работе через HTTPS использовать Gitea token/password согласно настройкам
  Gitea.
- SSH-ключи для Gitea настраиваются отдельно и не должны храниться в
  репозитории.
- GitHub остается публичным зеркалом и внешней площадкой; он не является
  primary registry source/build system в registry-readiness документации.
- При конфликте истории использовать `pull`/`rebase` только после ручной
  проверки расхождений.
- Перед release evidence фиксировать `git remote -v`, commit hash, tag,
  timestamp и источник сборки.
- Любые секреты, токены, приватные ключи и персональные данные не включать в
  repository evidence.

## Проверка HTTPS-контра

Ожидаемый публичный URL:

```text
https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus
```

Gitea должна обслуживаться через Nginx reverse proxy с HTTPS. После проверки
HTTPS внешний `3000/tcp` не должен быть доступен извне; локальный endpoint
Gitea фиксируется как `127.0.0.1:3000`.
