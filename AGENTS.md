# AGENTS.md

Operational rules for OpenCode/Codex agents in AWatch-rus.

## Defaults

- Rust is the primary runtime: use `adk-rust/`, build with `cargo build --release -p <crate>`, test with `cargo test -p <crate>`.
- Root scripts (`check-aw-data.sh`, `check-aw-full.sh`, `scripts/prod_rollout.sh`, install-kit helpers) are Rust-first wrappers with legacy fallback.
- Python is allowed only in `aw-server/dlp-content-analysis/`, `clickhouse-1c/ai/`, `clickhouse-1c/etl/`, `detmir-mcp/main.py`, `grafana-1c/`, `pfsense/`, `proxmox/tsj_guardian_bot.py`.
- Never add real secrets from `secrets/`, private `.env`, or host credentials.

## Required Checks

- General: `scripts/quality-gate.sh`.
- Rust: targeted `cargo test -p <crate>`.
- Windows: parse PowerShell; CI also runs PSScriptAnalyzer on `windows/*.ps1`, `.psm1`, `.psd1`.
- Ansible: affected `ansible-playbook --syntax-check ...`.

## Map

- `adk-rust/`: operational crates.
- `aw-server/`: server install, env examples, RU WebUI patch, systemd.
- `windows/`: RDP deployment, collectors, recovery, validation.
- `ansible/`: deployment playbooks.
- `proxmox/`: CT/gateway/bot automation.
- `clickhouse-1c/`, `grafana-1c/`, `pfsense/`: integration stacks.

## Entrypoints

Use `proxmox/create-ct.sh`, `proxmox/push-aw-artifacts.sh`, `aw-server/install_aw_server.sh`, `aw-server/apply_webui_ru_patch.sh`, `windows/deploy-ensemble.ps1`, and docs in `docs/preparation.md`, `docs/deployment.md`, `docs/runbook.md`, `docs/operations.md`.
