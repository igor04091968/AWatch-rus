# DetMir ADK-Rust Workspace

This directory is the Rust workspace for replacing operational Python and shell
scripts with durable standalone Rust modules.

## Layout

- `Cargo.toml` - workspace manifest and shared dependency versions.
- `Cargo.lock` - pinned dependency graph for reproducible builds.
- `crates/<module>` - one binary or library module per operational function.
- `target/` - local build output, ignored by git.

## Current Modules

- `detmir-auto` - no-heal autonomous orchestration shadow binary for running
  `detmir-check`, `detmir-dlp`, state/report writes, latest symlink updates, and
  retention cleanup.
- `detmir-core` - shared status levels, exit codes, and UTC timestamp helpers.
- `detmir-state` - DetMir autonomous state models, normalization, and atomic JSON writes.
- `detmir-aw-client` - small blocking ActivityWatch HTTP client and event timestamp helpers.
- `detmir-check` - read-only DetMir contour check replacement for the legacy Python command.
- `detmir-dlp` - SSH wrapper replacement for remote DLP health JSON collection.
- `dlp-health-check` - AW server DLP health check replacement.
- `aw-db-maintenance` - guarded weekly SQLite maintenance for old allowlisted
  process-level session events, with backup-before-delete.
- `aw-ensure-reliability` - safe dry-run/apply planner for AW service
  reliability repair actions that were previously immediate Bash mutations.
- `check-aw-full` - read-only local AW/RDP full check replacement for the
  legacy shell helper.
- `dlp-aggregator` - AW server DLP warehouse aggregator replacement.
- `dlp-influx-exporter` - AW server DLP InfluxDB line protocol exporter replacement.
- `worktime-autoheal` - AW server worktime autoheal and backfill replacement.
- `worktime-influx-exporter` - AW server worktime InfluxDB line protocol exporter replacement.
- `worktime-prewarm` - AW server worktime report cache prewarm replacement.
- `worktime-ui-bridge` - AW server worktime sessions to AFK/window bridge replacement.
- `dlp-syslog-forwarder` - AW server DLP syslog integration replacement.
- `dlp-webhook-sender` - AW server DLP webhook integration replacement.
- `dlp-cef-exporter` - AW server DLP CEF/syslog exporter replacement.
- `extract-ioc-from-sigma` - offline Sigma/Hayabusa IOC export replacement
  used by the DLP IOC preload wrapper.
- `merge-aw-server-dbs` - ActivityWatch SQLite DB merge replacement used by
  legacy root DB recovery/deploy tooling.
- `prod-backup-restore` - safe planner/checker for the destructive production
  backup-restore flow; apply remains explicit legacy-only at this stage.
- `prod-rollout` - safe planner/orchestrator for production AW server/Windows
  rollout; normal script runs are plan-only and real rollout requires
  explicit `--apply`.
- `rdp-worktime-report` - local RDP worktime CSV/JSON report helper
  replacement for the legacy shell/Python script.
- `aw-contour-smoke` - Rust replacement for the Proxmox-side DetMir contour
  smoke checks, used through a Rust-first project wrapper.
- `diag-and-manual-restart` - Rust replacement for the AW/DLP diagnostic and
  explicit manual restart helper, with conservative no-restart healthy path.
- `aw-browser-smoke` - Rust launcher for the browser smoke test; Playwright
  remains the execution engine and the legacy Node script remains fallback.
- `detmir-status` - read-only DetMir state normalizer with text, JSON, and ADK
  `Content` output. Also builds `detmir-adk-status` as a compatibility binary.

## Migration Runbook

Use `RUNBOOK.md` as the operational plan for replacing Python and shell modules
with Rust. It defines migration phases, safety gates, rollback rules, and the
order in which DetMir modules should be moved.

## Commands

```bash
cd adk-rust
cargo fmt --all
cargo check --workspace
cargo build --release --workspace
```

Run the current status module:

```bash
cd adk-rust
cargo run -p detmir-status -- --json
cargo run -p detmir-status -- --adk-json
cargo run -p detmir-status -- status --json
```

## Migration Rules

- New replacements go under `crates/` and are added to workspace `members`.
- Keep modules standalone: config comes from files, environment variables, or
  explicit CLI flags; no laptop-only assumptions.
- Default to read-only behavior first; add mutation/recovery paths only with
  tests and explicit operator-safe failure modes.
- Do not embed secrets in binaries, source files, or examples.
- Every module should expose machine-readable JSON output where practical.
