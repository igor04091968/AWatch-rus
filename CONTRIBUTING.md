# Contributing

## Branching

- Работайте в feature-ветке, не пушьте напрямую в `main`.
- Именование: `feat/...`, `fix/...`, `docs/...`, `chore/...`.

## Commit style

- Предпочтительно Conventional Commits:
  - `feat(...)`
  - `fix(...)`
  - `docs(...)`
  - `chore(...)`

## Required checks before PR

- `bash -n` для всех `*.sh`.
- `Invoke-ScriptAnalyzer` для `windows/*.ps1`, `windows/*.psm1`, `windows/*.psd1`.
- Проверка, что нет секретов (`secrets/deploy.secrets.env` не должен быть в индексе git).
- Обновлены инструкции и runbook при изменении поведения.

## PR content

- Изменения и обоснование.
- Риск и rollback.
- Какие команды валидации были выполнены.
