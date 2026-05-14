# Hayabusa Operator and IB Guide 2026-05-14

This document explains the role of Hayabusa inside `AW-rus` for operators and IB.

## What Hayabusa adds

Hayabusa adds a bounded DFIR layer for Windows Event Log analysis:

- EVTX-based timeline review
- Sigma-based detection enrichment
- logon and activity context around an already interesting host or incident
- forensic artifacts that can be attached to case review

It is useful when `AW-rus` or DLP already surfaced something worth investigating further.

## What Hayabusa does not replace

Hayabusa is not:

- a replacement for normal `AW-rus` runtime monitoring
- a replacement for DLP policy enforcement
- a real-time SIEM
- a reason to copy raw Sigma output into AW buckets or case comments

The normal operational path remains:

- `AW-rus` for activity/runtime visibility
- DLP collectors and policy engine for signal generation
- case management for operator workflow
- Hayabusa for bounded forensic enrichment

## When operators should run it

Recommended cases:

- high-severity DLP incidents
- repeated suspicious incidents on one host or user
- print, USB, file export, or email activity that needs Windows event corroboration
- investigation requests from IB after an incident is already known

Do not run it for every minor signal. It is meant for escalation and investigation, not daily noise.

## Operator workflow

1. Export EVTX package on Windows:

```powershell
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1
```

2. Transfer the resulting zip package to `10.10.10.13`.

3. Run server-side processing:

```bash
aw-hayabusa accept --package /path/to/HOST-YYYYMMDD-HHMMSS.zip --host HOST
aw-hayabusa process-inbox --mode incident
```

Or use the Telegram operator path:

```text
/aw_dfir /path/to/HOST-YYYYMMDD-HHMMSS.zip HOST [CASE_ID] [MODE]
```

4. If a case already exists, link only bounded metadata to the case.

## Where artifacts live

Windows export staging:

- `C:\ProgramData\AWatch-rus\forensics\evtx-exports`

Server-side intake and reports:

- incoming packages:
  - `/opt/hayabusa/inbox/incoming`
- transient staging:
  - `/opt/hayabusa/inbox/staging`
- archived raw packages:
  - `/opt/hayabusa/archive/packages/<HOST>/`
- archived extracted payloads:
  - `/opt/hayabusa/archive/extracted/<HOST>/<INTAKE_ID>/payload/`
- reports:
  - `/opt/hayabusa/reports/<HOST>/<UTC_TIMESTAMP>_<MODE>[_LABEL]/`
- run state and logs:
  - `/opt/hayabusa/state`

## What is stored in AW-rus

Only bounded metadata is attached to a case:

- tool
- host
- mode
- status
- intake id
- package path
- sha256
- report directory
- summary path
- timeline path
- manifest path
- linked timestamp
- link source

Raw forensic output stays under `/opt/hayabusa`.

## Retention and storage notes

Windows-side export retention:

- controlled by `aw_windows_evtx_retention_days`
- default: `14` days

Windows-side export channels:

- controlled by `aw_windows_evtx_channels`
- default set:
  - `Security`
  - `System`
  - `Application`
  - `Microsoft-Windows-PowerShell/Operational`
  - `Microsoft-Windows-TerminalServices-LocalSessionManager/Operational`
  - `Microsoft-Windows-TerminalServices-RemoteConnectionManager/Operational`

Server-side storage:

- kept outside standard AW buckets
- kept outside normal DLP screenshot artifacts
- intended for forensic review, not for routine dashboarding

## IB view

From an IB perspective, Hayabusa in this project is:

- a post-incident enrichment layer
- useful for Windows event corroboration and timeline reconstruction
- intentionally separated from the main activity-monitoring data plane

This design keeps the main operator UI readable while preserving forensic detail when needed.

## Limits and false expectations to avoid

- Sigma detections depend on the quality and completeness of Windows logging.
- Missing or weak audit policy reduces value immediately.
- No EVTX means no meaningful Hayabusa result.
- A successful Hayabusa run does not prove malicious activity by itself.
- A clean Hayabusa run does not prove the absence of suspicious behavior.
- This contour is deliberately not an always-on detector and not a SIEM replacement.

## Canonical companion docs

- source and packaging:
  - `docs/hayabusa-source-packaging-2026-05-14.md`
- server runner:
  - `docs/hayabusa-server-runner-2026-05-14.md`
- artifact workflow:
  - `docs/hayabusa-artifact-workflow-2026-05-14.md`
- AW-rus integration:
  - `docs/hayabusa-aw-rus-integration-2026-05-14.md`
- Windows EVTX export:
  - `docs/windows-hayabusa-evtx-export.md`
