# Hayabusa Artifact Workflow 2026-05-14

This document records the server-side EVTX intake and archive workflow on `10.10.10.13`.

## Directories

- incoming packages:
  - `/opt/hayabusa/inbox/incoming`
- transient staging:
  - `/opt/hayabusa/inbox/staging`
- generated reports:
  - `/opt/hayabusa/reports`
- archived raw packages:
  - `/opt/hayabusa/archive/packages/<HOST>/`
- archived extracted payloads:
  - `/opt/hayabusa/archive/extracted/<HOST>/<INTAKE_ID>/payload/`
- state:
  - `/opt/hayabusa/state/latest-intake.json`
  - `/opt/hayabusa/state/latest-run`
  - `/opt/hayabusa/state/latest-<HOST>`
  - `/opt/hayabusa/state/logs`

## Operator flow

1. Accept a package into server inbox:

```bash
aw-hayabusa accept --package /path/to/HOST-YYYYMMDD-HHMMSS.zip --host HOST
```

2. Inspect queue:

```bash
aw-hayabusa inventory
```

3. Process queued packages:

```bash
aw-hayabusa process-inbox --mode incident
```

## Processing behavior

- the package is extracted into staging;
- host is resolved from explicit `--host`, sidecar `.host`, embedded `manifest.json`, or package name fallback;
- if EVTX payload exists, Hayabusa analysis is launched through the existing runner modes;
- regardless of success, the package and extracted payload are moved into archive;
- `intake.json` records:
  - package path
  - host
  - intake id
  - sha256
  - status
  - extracted payload path
  - report directory
  - processed timestamp

## Failure semantics

- malformed or empty packages are not lost;
- the workflow archives them with `status=failed-*`;
- the operator can inspect archived payloads without touching AW runtime storage.

## Validation evidence

- `aw-hayabusa inventory` shows queue and archive counts
- a synthetic package was accepted, archived, and recorded with:
  - `status=failed-no-evtx`
- synthetic artifacts were removed after validation so production storage stayed clean

## Boundaries

- this phase does not yet move packages from Windows automatically
- this phase does not yet attach reports to AW-rus incidents or cases
- successful report generation from real EVTX remains a later validation phase
