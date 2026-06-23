# Runbook создания публичных GitHub issues

Дата: 2026-06-23

Статус: issue templates готовы; 12 публичных GitHub issues созданы, а ссылки
записаны в `docs/public-issues/public-issues-manifest.json`. Этот runbook
остается процедурой для повторного, добавочного или ручного создания issues.

GitHub issues используются для public roadmap visibility. Они не являются
registry release evidence. Primary registry contour остается Russian Gitea +
planned Russian build-runner.

## Подготовленный пакет

- Issue templates: `docs/public-issues/*.md`.
- Manifest: `docs/public-issues/public-issues-manifest.json`.
- Dry-run check: `scripts/prepare_public_issues.sh`.
- Opt-in creation script: `scripts/create_public_issues_from_manifest.sh`.

## Ручное создание через GitHub UI

1. Открыть GitHub repository issue tracker.
2. Для каждого файла `docs/public-issues/NNN-*.md` создать новый issue.
3. Взять `Title` из секции `## Title`.
4. Скопировать тело issue из markdown-файла целиком.
5. Назначить labels из секции `## Labels`.
6. Проверить, что в тексте нет секретов, персональных данных, реальных
   customer identifiers, внутренних IP/hostname и логов сотрудников.
7. После публикации скопировать URL issue.
8. Обновить `github_issue_url` в
   `docs/public-issues/public-issues-manifest.json`.

## Создание через gh CLI

Dry-run:

```bash
bash scripts/prepare_public_issues.sh
```

Скрипт проверяет наличие файлов, обязательные секции и manifest. Он не требует
GitHub token и не создает issues.

Opt-in создание:

```bash
gh auth status
CONFIRM_CREATE_GITHUB_ISSUES=YES bash scripts/create_public_issues_from_manifest.sh
```

Скрипт:

- требует `CONFIRM_CREATE_GITHUB_ISSUES=YES`;
- требует `gh` и `jq`;
- проверяет `gh auth status`;
- создает отсутствующие labels;
- создает issues по manifest;
- печатает URL созданных issues для последующего ручного внесения в manifest.

Скрипт не запускается из `scripts/registry_readiness_check.sh`.

## Labels

Ожидаемые labels:

- `registry`
- `ops`
- `evidence`
- `build-runner`
- `release`
- `legal`
- `docs`
- `qa`
- `coverage`
- `policy`
- `security`
- `review`
- `governance`
- `compat`
- `demo`
- `public`
- `pilot`
- `process`
- `github`

## Обновление manifest после создания

До создания:

```json
"github_issue_url": null
```

После создания:

```json
"github_issue_url": "https://github.com/igor04091968/AWatch-rus/issues/<number>"
```

Для созданного issue manifest должен фиксировать:

```json
{
  "status": "created",
  "github_issue_url": "https://github.com/igor04091968/AWatch-rus/issues/<number>",
  "created_at": "YYYY-MM-DDTHH:MM:SSZ",
  "created_by": "maintainer"
}
```

Если issue не создан, `status` остается `ready_to_create`, а
`github_issue_url` остается `null`.

## Запрещенные данные

В публичные issues нельзя вставлять:

- пароли, tokens, private keys, recovery codes;
- реальные IP, hostname, VPN details или private network topology;
- ФИО сотрудников, логи сотрудников, screenshots с персональными данными;
- customer identifiers, contract data или private legal evidence;
- security exploit details до triage по `SECURITY.md`.

## Forbidden claims

Issues не должны утверждать:

- Do not claim completed Russian software registry submission.
- Do not claim FSTEC/FSB certification.
- Do not claim SIEM/DLP replacement.
- Forbidden claim: ML/LLM-based detection is not claimed.
- Forbidden claim: automatic remediation is not claimed.
- Do not claim active external peer review until public reviewed PRs exist.
- Do not claim enabled branch protection until repository settings are verified.
- Do not claim ready Russian build-runner until provisioning evidence exists.
- Do not claim completed restore test until restore evidence exists.

## Проверки перед commit

```bash
python3 scripts/public_secret_pattern_check.py
bash -n scripts/prepare_public_issues.sh
bash scripts/prepare_public_issues.sh
bash -n scripts/create_public_issues_from_manifest.sh
bash -n scripts/registry_readiness_check.sh
bash scripts/registry_readiness_check.sh
git diff --check
```
