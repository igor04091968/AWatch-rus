# Windows Deploy Startup Model

## Supported startup models

### 1. Multi-user RDP host

Use this model on `SHARKON2025`-style hosts with multiple user sessions.

- `ActivityWatch Launch [HOST_user]` tasks:
  - `AtLogOn`
  - `InteractiveToken`
  - start only for users that currently have a real Windows session
- `ActivityWatch Recovery` task:
  - `AtStartup`
  - `SYSTEM`
  - keeps only the global `worktime-session-collector` alive
  - may re-trigger user launch tasks, but only for users whose sessions currently exist
- interactive collectors/watcher binaries belong to the user-session path, not to Session 0

Collector ownership in this model:

- `aw-watcher-afk` and `aw-watcher-window`: user-session only
- `browser-domains-native-collector.ps1`: user-session only
- `email-outbound-collector.ps1`: user-session only
- `file-operations-collector.ps1`: user-session path
- `dlp-endpoint-signals-collector.ps1`: user-session path
- `worktime-session-collector.ps1`: single global process under recovery path

### 2. Standalone service installer

Use this model on single-user or headless hosts where Task Scheduler per-user orchestration is not the primary control plane.

- `aw-standalone-service.ps1` runs as a loop/service wrapper
- Session 0 starts only collectors that are safe headless
- browser/email interactive collectors must not be assumed available from Session 0

Collector ownership in this model:

- `dlp-endpoint-signals-collector.ps1`: allowed
- `file-operations-collector.ps1`: allowed
- `worktime-session-collector.ps1`: allowed
- `browser-domains-native-collector.ps1`: not reliable in Session 0
- `email-outbound-collector.ps1`: not reliable in Session 0
- `aw-watcher-afk` / `aw-watcher-window`: not a standalone Session 0 primitive

## Non-supported mix

Do not mix the two startup models on the same RDP host:

- no permanent standalone-service loop together with per-user launch/recovery tasks
- no blind `Start-ScheduledTask` for all configured users
- no validation rule that treats users without sessions as failed collector startup

## Hardening rules

- start launch tasks only for users with real sessions
- keep only one global `worktime-session-collector`
- validate by session-aware expectations, not by “all configured users must currently run”
- keep `deploy_aw_windows.yml`, `deploy-ensemble.ps1`, `hardening-recovery.ps1`, and installer assumptions aligned
