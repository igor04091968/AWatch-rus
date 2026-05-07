# Copilot instructions for ActivityWatch-Russian

Purpose: help future Copilot sessions quickly understand how to build, validate, and modify this repo.

---

## Build / test / lint (how-to)

- Shell script checks (CI & local):
  - Full: ./scripts/quality-gate.sh
  - Single file (syntax): bash -n <script>. Example: bash -n scripts/install_aw_linux_client.sh
  - Run shellcheck locally (same checks as CI): install shellcheck then run:
    find . -type f -name "*.sh" -print0 | xargs -0 -r shellcheck -e SC1007,SC1090,SC2016

- PowerShell checks (Windows / CI):
  - Single-file analysis (locally in PowerShell):
    Invoke-ScriptAnalyzer -Path windows/deploy-ensemble.ps1
  - CI installs PSScriptAnalyzer and runs against windows/*.ps1, *.psm1, *.psd1

- Python scripts / utilities:
  - Run a single utility: python3 scripts/aggregate_dlp_events.py
  - Many scripts are helpers for operations; no test harness in repo.

- Monitoring stack (Docker Compose):
  - Start: cd grafana-1c && docker-compose up -d
  - Start a single service: docker-compose up -d grafana

- Server install / deploy helpers:
  - AW server install: aw-server/install_aw_server.sh
  - Apply RU WebUI patch: aw-server/apply_webui_ru_patch.sh
  - Windows deploy/validation: windows/deploy-ensemble.ps1 and windows/validate-deployment.ps1

Notes: there is no unified unit-test suite. Use the script checks and CI pipeline (.github/workflows/ci.yml) as the canonical validation steps.

---

## High-level architecture (short)

- Windows collectors (PowerShell) run on endpoints and POST events to the ActivityWatch Server HTTP API.
- ActivityWatch Server (deployed on Linux CT/LXC via Proxmox or Debian VM) stores events in PostgreSQL and serves WebUI.
- Integration layer: pollers and aggregators (Python) for pfSense, DLP aggregation, Prometheus exporter.
- Monitoring: Prometheus + Grafana (docker-compose in grafana-1c) and a SQL exporter for direct DB dashboards.

Key ports: AW API 5600/5666, PostgreSQL 5432, Prometheus 9090, Grafana 3000, exporter 9398.

---

## Key repository conventions

- Branching / commits:
  - Use feature branches. Commit style follows Conventional Commits (feat/fix/docs/chore).

- Secrets and envs:
  - Secrets live in secrets/*.env templates and must NOT be committed. Use secrets/deploy.secrets.env locally; CI and scripts expect templates (.example).

- Preflight / PR checks:
  - Run bash -n for shell scripts and Invoke-ScriptAnalyzer for PowerShell before opening PRs.
  - Update docs/runbook.md and related runbooks when behavior changes.

- RU patching:
  - WebUI localization is applied via aw-server/aw-ru-patch.js and aw-server/apply_webui_ru_patch.sh — treat these as idempotent patch steps during deploy.

- Systemd / deploy units:
  - activitywatch-server.service / aw-worktime-api.service / aw-worktime-ui-bridge.service are included in aw-server/ for production use.

- CI expectations:
  - .github/workflows/ci.yml runs shellcheck and PSScriptAnalyzer. Use scripts/quality-gate.sh locally to replicate preflight.

---

## Important files & quick references

- docs/ (onboarding, deployment, runbook) — start here for operational context.
- aw-server/ — server install script, env template, RU patch, systemd units.
- ansible/ — automated provisioning playbooks for CT/Proxmox and Windows deploys.
- windows/ — PowerShell collectors and orchestration; validation scripts are here.
- scripts/ — helpers (aggregate_dlp_events.py, installers, quality-gate.sh).
- grafana-1c/ — docker-compose monitoring stack and dashboards.

---

## AI assistant & other tool configs to check

- No Copilot-specific instruction file existed before this addition.
- No CLAUDE.md, .cursorrules, AGENTS.md, .windsurfrules, CONVENTIONS.md, or AIDER_CONVENTIONS.md detected at repo root. If adding automated assistant rules, place them in repo root or .github and document cross-references here.

---

If you need the Copilot instructions extended (e.g., adding run examples for specific scripts, more detailed CI breakdown, or mapping tests to files), say which area to expand.
