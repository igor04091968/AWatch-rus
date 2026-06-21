# Runbook: российский Git-контур и зеркалирование

Статус: operational registry-readiness runbook. Команды ниже описывают
текущую рабочую схему remotes для российского self-hosted Gitea-дубликата и
публичного GitHub mirror.

## Текущая схема remotes

Primary registry-readiness remote / Gitea duplicate:

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

На текущей локальной рабочей копии этой машины `origin` указывает на GitHub:

```text
origin:
https://github.com/igor04091968/AWatch-rus.git
```

Для прямой синхронизации с Gitea нужно добавить отдельный remote `ru-origin`.

## Доступ и секреты

- Gitea operator account: `igor`.
- Пароль, personal access token, SSH private key и recovery codes не хранить в
  репозитории, `docs/registry/`, Gitea Wiki или release evidence.
- Для HTTPS push предпочтительно использовать Gitea personal access token или
  credential helper. Если используется пароль учетной записи, он должен
  оставаться только в приватном хранилище учетных данных.
- Перед публикацией evidence проверять, что выводы команд не содержат секреты.

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
  Gitea, без записи секрета в tracked files.
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
