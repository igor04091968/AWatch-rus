AGENTS for OpenCode

Keep this file minimal and high-signal: only include facts an agent would otherwise miss.

1) Repo purpose (one line)
- This repository bundles an ActivityWatch Server deployment, RU WebUI patch, Windows collectors (PowerShell), and small Python utilities for aggregation and monitoring.

2) Highest-value entrypoints & commands
- Read README.md and docs/preparation.md first (they are the authoritative onboarding flow).
- Create a Proxmox CT: `proxmox/create-ct.sh [ /path/to/deploy.secrets.env ]` (reads secrets/deploy.secrets.env).
- Push server artifacts to an existing CT: `proxmox/push-aw-artifacts.sh [ /path/to/deploy.secrets.env ]`.
- Install AW server on the CT (runs inside CT): `aw-server/install_aw_server.sh` (requires `/etc/activitywatch/aw-server.env`).
- Apply RU WebUI patch (must run on the CT and after webui is present): `aw-server/apply_webui_ru_patch.sh`.
- Run the Windows ensemble deploy from a Windows admin host: `windows/deploy-ensemble.ps1` (see its parameters; it calls `deploy-domain-users.ps1`).
- Quick DLP aggregation (local): `python3 scripts/aggregate_dlp_events.py`.

3) Exact env/secrets behavior agents often miss
- Secrets live in `secrets/deploy.secrets.env` (actual file is intentionally local-only). Many scripts default to that path if no arg provided. Never add real secrets to commits. Use `.example` files as templates.
- The CT bootstrap workflow expects `/etc/activitywatch/aw-server.env` on the CT (pushed by push-aw-artifacts when AW_SERVER_* variables are set). `install_aw_server.sh` sources that exact path.

4) CI / quality checks the repo enforces
- GitHub CI runs shellcheck for `*.sh` and PSScriptAnalyzer for PowerShell in `windows/*.ps1` (see .github/workflows/ci.yml).
- Local quality gate: `scripts/quality-gate.sh` performs bash `-n`, (optional) shellcheck, pwsh parse checks, and ansible syntax checks. Run this before PRs.

5) File locations and toolchain quirks
- AW server binary is installed under `/opt/activitywatch/releases` and symlinked from `/opt/activitywatch/bin/aw-server-rust` by `install_aw_server.sh`.
- RU WebUI patching expects JS assets in `$AW_SERVER_WEBUI_DIR` (default `/opt/activitywatch/webui-ru`). The patch script writes `js/ru-patch-v5.js` and edits `index.html` in-place (it makes backups with .bak timestamps).
- `proxmox/create-ct.sh` and `proxmox/push-aw-artifacts.sh` source the same deploy.secrets.env and require many CT_* / AW_SERVER_* variables; missing vars cause immediate exit.

6) Monorepo boundaries / responsibilities
- aw-server/: server install & systemd unit + RU webui patching.
- proxmox/: create CT and push artifacts scripts (requires Proxmox `pct` CLI and CT preconditions).
- windows/: PowerShell collectors and deployment automation (Target: Windows admin hosts; validated via `validate-deployment.ps1`).
- grafana-1c/, pfsense/: monitoring stacks and pollers (separate deploys, not part of aw-server install).
- scripts/: small utilities and `quality-gate.sh` used by contributors.

7) Common gotchas
- Do NOT commit secrets (secrets/ are local-only; PRs must not contain real secrets).
- Many scripts assume they run on the target CT or on a Linux admin host with `pct` available. Don't try to run them on macOS without adapting dependencies.
- `aw-server/install_aw_server.sh` expects network access to download the AW release URL provided by AW_SERVER_DOWNLOAD_URL.
- `aw-server/apply_webui_ru_patch.sh` must run after AW webui files are present; it will fail if required bootstrap files under `/root/bootstrap` are missing.
- When pushing AW_SERVER env via `push-aw-artifacts.sh` the script will only write `/etc/activitywatch/aw-server.env` if all AW_SERVER_* variables are set; otherwise it warns and skips.

8) PR / commit checklist for agents
- Run `scripts/quality-gate.sh` locally (or ensure CI covers changed files).
- Ensure no secrets (.env with real values) are staged.
- If changing PowerShell, ensure PSScriptAnalyzer rules pass (CI enforces this).

9) Where to find more instructions (preserve these files)
- README.md, docs/preparation.md, docs/deployment.md, docs/runbook.md, docs/operations.md, docs/codebase-onboarding.md — read these when doing infra or deployment work.

If you need me to add step-by-step repros or automate one of the tasks above (create CT, push artifacts, run install on CT), say which one and I will implement the helper or run the checked commands.
