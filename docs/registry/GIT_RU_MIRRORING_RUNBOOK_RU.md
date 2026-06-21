# Runbook: российский Git-контур и зеркалирование

Статус: operational registry-readiness runbook. Команды ниже описывают
целевую схему remotes для работы с российским self-hosted Gitea и публичным
GitHub mirror.

## Целевая схема remotes

Primary Russian Git contour:

```text
ru-origin:
https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus.git
```

Public mirror / external public repository:

```text
github:
git@github.com:igor04091968/AWatch-rus.git
```

или:

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

## Операционные предупреждения

- При работе через HTTPS использовать Gitea token/password согласно настройкам
  Gitea.
- SSH-ключи для Gitea настраиваются отдельно и не должны храниться в
  репозитории.
- GitHub остается публичным зеркалом и внешней площадкой; он не является
  primary registry source/build system в registry-readiness документации.
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
