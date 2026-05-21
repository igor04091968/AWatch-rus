# Windows EVTX Export for Hayabusa

This document defines the Windows-side export path for Hayabusa DFIR enrichment.

## Purpose

Windows hosts do not analyze EVTX locally for this contour.

They export selected event logs into a bounded forensic staging area, and the server-side Hayabusa workflow on `10.10.10.13` analyzes those artifacts later.

## Export script

- script: `windows/export-evtx-for-hayabusa.ps1`
- deployed path on Windows host:
  - `<StateRoot>\export-evtx-for-hayabusa.ps1`

Default config path:

- `C:\ProgramData\AWatch-rus\deployment-config.json`

## Default export root

- `<StateRoot>\forensics\evtx-exports`
- Ansible override variable: `aw_windows_forensics_root`
- retention override variable: `aw_windows_evtx_retention_days`
- channel override variable: `aw_windows_evtx_channels`

Example:

- `C:\ProgramData\AWatch-rus\forensics\evtx-exports`

Each run creates:

- `<forensics-root>\<HOST>-<YYYYMMDD-HHMMSS>\evtx\*.evtx`
- `<forensics-root>\<HOST>-<YYYYMMDD-HHMMSS>\manifest.json`
- optional zip:
  - `<forensics-root>\<HOST>-<YYYYMMDD-HHMMSS>.zip`

## Default channel set

- `Security`
- `System`
- `Application`
- `Microsoft-Windows-PowerShell/Operational`
- `Microsoft-Windows-TerminalServices-LocalSessionManager/Operational`
- `Microsoft-Windows-TerminalServices-RemoteConnectionManager/Operational`

Notes:

- `Sysmon` is intentionally not assumed by default.
- If `Sysmon` exists in the environment, it should be added later as an explicit extension.
- the channel list is now carried through deployment config and validation, not left as an implicit script default.

## Retention

- default retention: `14` days
- cleanup is local to the forensic export root
- old export directories and zip packages are removed after the retention cutoff
- retention is now exposed as `aw_windows_evtx_retention_days` in Ansible vars

## Example run

```powershell
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1
```

Example with custom window:

```powershell
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1 -DaysBack 1
```

Current production path:

```powershell
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-upload-hayabusa-to-aw-server.ps1 -HoursBack 6 -CaseId 30
```

This wrapper:

- builds the EVTX package;
- uploads `.caseid` if provided;
- uploads `.meta.json`;
- uploads the `zip` to the AW-server drop directory.

## Boundaries

- output stays outside standard AW buckets
- output stays outside normal DLP screenshot artifacts
- server-side Hayabusa execution happens later on `10.10.10.13`
- only bounded Hayabusa metadata returns into the case layer
