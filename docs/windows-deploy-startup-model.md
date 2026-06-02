# Windows Deploy Startup Model

## Supported startup models

### 1. Multi-user RDP host

Use this model on `SHARKON2025`-style hosts with multiple user sessions.

- `AWatchRusCollectorGuard` service:
  - runs under `LocalSystem`
  - is the preferred local control plane for collector supervision
  - in `shadow` mode only publishes state/heartbeat
  - in `enforce` mode starts only session-appropriate collectors/tasks with cooldown and restart budget
  - publishes `aw-rus-collector-guard_<HOST>`
- `ActivityWatch Launch [HOST_user]` tasks:
  - `AtLogOn`
  - `InteractiveToken`
  - start only for users that currently have a real Windows session
- `ActivityWatch Recovery` task:
  - `AtStartup`
  - `SYSTEM`
  - stays enabled even when `AWatchRusCollectorGuard` is active
  - keeps only the global `worktime-session-collector` alive
  - may re-trigger user launch tasks for managed live or disconnected sessions
- interactive collectors/watcher binaries belong to the user-session path, not to Session 0

Collector ownership in this model:

- `aw-watcher-afk` and `aw-watcher-window`: user-session only
- `browser-domains-native-collector.ps1`: user-session only
- `email-outbound-collector.ps1`: user-session only
- `file-operations-collector.ps1`: user-session path
- `dlp-endpoint-signals-collector.ps1`: user-session path
- `worktime-session-collector.ps1`: single global process under recovery path
  - publishes session presence and `process_start` / `process_stop` events for all visible user sessions, including `Disc`
  - this is session/process telemetry, not a replacement for per-user foreground window watchers

### 2. Standalone service installer

Use this model on single-user or headless hosts where Task Scheduler per-user orchestration is not the primary control plane.

- `aw-standalone-service.ps1` runs as a loop/service wrapper
- Session 0 starts only collectors that are safe headless
- browser/email interactive collectors must not be assumed available from Session 0

Collector ownership in this model:

- `dlp-endpoint-signals-collector.ps1`: allowed
- `file-operations-collector.ps1`: allowed
- `worktime-session-collector.ps1`: allowed
  - still provides session/process telemetry, but not interactive foreground-window truth
- `browser-domains-native-collector.ps1`: not reliable in Session 0
- `email-outbound-collector.ps1`: not reliable in Session 0
- `aw-watcher-afk` / `aw-watcher-window`: not a standalone Session 0 primitive

## Non-supported mix

Do not mix the two startup models on the same RDP host:

- no permanent standalone-service loop together with per-user launch/recovery tasks
- no blind `Start-ScheduledTask` for all configured users
- no validation rule that treats users without sessions as failed collector startup
- no bot-driven collector recovery as the primary control plane

During migration, `AWatchRusCollectorGuard` may run in `shadow` mode beside the existing
recovery task. In `enforce` mode it becomes the primary control plane; `ActivityWatch Recovery`
still remains enabled as fallback/bootstrap and must not be disabled by deploy scripts.

## Hardening rules

- start launch tasks only for users with real sessions
- treat managed disconnected RDP sessions as real recovery targets when their launch tasks exist
- keep only one global `worktime-session-collector`
- validate by session-aware expectations, not by “all configured users must currently run”
- use guard heartbeat and bucket freshness as health signals
- keep `deploy_aw_windows.yml`, `deploy-ensemble.ps1`, `hardening-recovery.ps1`, and installer assumptions aligned
