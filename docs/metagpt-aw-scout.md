# MetaGPT AW Scout

`scripts/metagpt-aw-scout.sh` is the safe project wrapper for using the
MetaGPT-configured LLM provider with ActivityWatch-Russian tasks.

Default mode is direct LLM scout through `/home/igor/.metagpt/config2.yaml`.
This avoids MetaGPT Browser/Editor tools, which are too noisy for operational
checklists.

Optional MetaGPT team mode is still available:

```bash
METAGPT_AW_ENGINE=team scripts/metagpt-aw-scout.sh smoke
```

Do not use full `--implement` mode with the free Groq tier for this project. It
pulls too much context and usually exceeds TPM limits.

## Setup

Use a valid provider key outside git:

```bash
export GROQ_API_KEY="gsk_..."
```

The global wrapper `/home/igor/bin/metagpt-lab` can write the key into
`/home/igor/.metagpt/config2.yaml`. The scout script reads that config and does
not print secrets.

## Presets

```bash
scripts/metagpt-aw-scout.sh qa-rollback
scripts/metagpt-aw-scout.sh smoke
scripts/metagpt-aw-scout.sh grafana
scripts/metagpt-aw-scout.sh install-kit
scripts/metagpt-aw-scout.sh windows-i18n
```

Free-form task:

```bash
scripts/metagpt-aw-scout.sh "Review risk of changing aw-worktime-ui-bridge foreground cache"
```

Reports are saved under:

```text
.ai/metagpt/
```

## Current Rule

The LLM is only a scout. Codex/operator must verify every material claim against
local files, live services, Ansible output, Grafana, and ActivityWatch APIs
before changing production behavior. Use `METAGPT_AW_ENGINE=team` only for
experiments; direct mode is the operational default.
