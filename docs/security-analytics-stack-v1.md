# AW-rus Security Analytics Stack v1

## Goal

Build a sufficient internal security analytics stack for the current environment without pretending to be Splunk-class infrastructure.

## Scope

Sources:
- Windows EVTX
- ActivityWatch buckets
- DLP incidents
- file operations
- outbound email
- session/logon markers

Core outcomes:
- ingest
- normalize
- detect
- correlate
- case
- notify
- investigate

## v1 Architecture

### Windows side

- collectors and DLP scripts write to `deployment-config.json`
- `export-evtx-for-hayabusa.ps1` exports bounded EVTX packages
- `export-upload-hayabusa-to-aw-server.ps1` uploads:
  - `zip`
  - `.meta.json`
  - optional `.caseid`
- `ActivityWatch Hayabusa Upload` scheduled task runs every 6 hours

### Server side

- `aw-hayabusa-drop.path` watches `/opt/activitywatch/aw-rus-ops/drop`
- `aw-hayabusa-drop.service` runs `aw-hayabusa-autoprocess`
- `aw-hayabusa` performs:
  - accept
  - process-inbox
  - report generation
- `aw-hayabusa-case-alert` performs:
  - severity scoring from `timeline.jsonl`
  - optional auto-case creation
  - bounded Hayabusa linkage
  - summary comment
  - Telegram alerting

### Case layer

- DLP case API remains the source of truth for incident lifecycle
- Hayabusa writes bounded metadata into `forensics.hayabusa`
- auto-created cases use:
  - `incident_id = hayabusa:<host>:<intake_id>`

## Severity Model v1

Inputs:
- Hayabusa `Level`
- top `RuleTitle`
- failed logon count
- suspicious PowerShell count
- credential-related detections
- timestomp detections

Outputs:
- `low`
- `medium`
- `high`
- `critical`

Rules:
- `critical` for `crit` alerts, very high score, or strong compound signals
- `high` for at least one high alert or elevated score
- `medium` for med alerts, notable failed logons, or moderate score
- `low` otherwise

## Automation Policy v1

- EVTX upload every 6 hours
- lookback window: 6 hours
- auto-case enabled from `medium`
- Telegram enabled from `high`
- human operator only for final triage/escalation

## Non-goals

Not trying to implement:
- distributed search cluster
- Splunk-style indexers/search heads
- full SIEM content ecosystem
- petabyte-scale retention design

## Definition of Done

The stack is sufficient when it can, without a dedicated analyst:
- collect relevant data
- process EVTX on schedule
- score severity
- create/update a case
- send an alert
- preserve investigation artifacts
- let a human understand what happened in a few minutes
