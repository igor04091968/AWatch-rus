# Hayabusa Server-Side Runner 2026-05-14

This document records the production runner model for Hayabusa on `10.10.10.13`.

## Install layout

- root: `/opt/hayabusa`
- pinned release: `/opt/hayabusa/releases/v3.9.0`
- active symlink: `/opt/hayabusa/current`
- operator entrypoint: `/usr/local/bin/aw-hayabusa`

## Runtime directories

- inbox: `/opt/hayabusa/inbox`
- archive: `/opt/hayabusa/archive`
- reports: `/opt/hayabusa/reports`
- state: `/opt/hayabusa/state`

## Operator entrypoint

Supported helper subcommands:

- `aw-hayabusa doctor`
- `aw-hayabusa inventory`
- `aw-hayabusa accept --package <zip> [--host HOST]`
- `aw-hayabusa process-inbox [--mode incident] [--limit N]`
- `aw-hayabusa profiles`
- `aw-hayabusa version`

Supported analysis modes:

- `aw-hayabusa quick --input <file-or-dir> [--host HOST]`
- `aw-hayabusa incident --input <file-or-dir> [--host HOST]`
- `aw-hayabusa full --input <file-or-dir> [--host HOST]`

## Mode intent

- `quick`
  - fast CSV timeline
  - HTML summary
  - logon summary
  - intended for first-pass triage

- `incident`
  - JSONL timeline
  - HTML summary
  - logon summary
  - intended for normal incident review

- `full`
  - JSONL timeline
  - deprecated/noisy/unsupported rules enabled
  - HTML summary
  - logon summary
  - intended for deeper DFIR review

## Output naming

Reports are stored under:

- `/opt/hayabusa/reports/<HOST>/<UTC_TIMESTAMP>_<MODE>[_LABEL]/`

Typical contents:

- `timeline.csv` or `timeline.jsonl`
- `summary.html`
- `logon-summary-*.csv`
- `run.log`
- `manifest.json`

Latest-run symlinks:

- `/opt/hayabusa/state/latest-run`
- `/opt/hayabusa/state/latest-<HOST>`

## Intake and archive workflow

Incoming packages:

- `/opt/hayabusa/inbox/incoming/*.zip`

Transient staging:

- `/opt/hayabusa/inbox/staging/<INTAKE_ID>/`

Archived raw packages:

- `/opt/hayabusa/archive/packages/<HOST>/<INTAKE_ID>.zip`

Archived extracted payloads:

- `/opt/hayabusa/archive/extracted/<HOST>/<INTAKE_ID>/payload/`
- intake metadata:
  - `/opt/hayabusa/archive/extracted/<HOST>/<INTAKE_ID>/intake.json`

State/log helpers:

- `/opt/hayabusa/state/latest-intake.json`
- `/opt/hayabusa/state/logs/`

## Minimal operator flow

1. Drop or copy an export package:
   - `aw-hayabusa accept --package /path/to/HOST-YYYYMMDD-HHMMSS.zip`
2. Check queue:
   - `aw-hayabusa inventory`
3. Process packages:
   - `aw-hayabusa process-inbox --mode incident`

## Validation baseline

Minimum server-side validation:

```bash
aw-hayabusa doctor
aw-hayabusa profiles
aw-hayabusa inventory
```

## Boundaries

- Hayabusa is not deployed as a daemon.
- No AW bucket ingestion happens in this phase.
- EVTX intake orchestration remains a later phase.
