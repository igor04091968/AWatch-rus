# aw-rus Hayabusa Server Ops Bundle

This directory is the server-side operational bundle for Hayabusa on `192.0.2.13`.

## Goal

Allow operators to run the full bounded DFIR path without depending on the laptop repository.

## Server paths

- wrapper: `/usr/local/bin/aw-hayabusa`
- case linker: `/usr/local/bin/aw-hayabusa-link-case`
- Windows-driven E2E helper: `/usr/local/bin/aw-hayabusa-from-windows`
- ops bundle root: `/opt/activitywatch/aw-rus-ops`
- local inventory for server-side controller mode: `/opt/activitywatch/aw-rus-ops/ansible/inventory.ini`
- local controller venv: `/opt/activitywatch/aw-rus-ops/venv`
- local drop zone for fetched EVTX zips: `/opt/activitywatch/aw-rus-ops/drop`

## Minimal operator workflow on the server

1. Check runner health:

```bash
aw-hayabusa doctor
aw-hayabusa inventory
```

2. If a zip is already on the server:

```bash
aw-hayabusa accept --package /path/to/HOST-YYYYMMDD-HHMMSS.zip --host HOST
aw-hayabusa process-inbox --mode incident
```

3. Link the latest successful run to a case:

```bash
aw-hayabusa-link-case --case-id 30 --mode incident
```

## Full no-laptop workflow from the server

Prerequisites:

- `/opt/activitywatch/aw-rus-ops/venv` contains `ansible` and `pywinrm`
- `/opt/activitywatch/aw-rus-ops/ansible/inventory.ini` contains the live Windows connection details
- WinRM from `192.0.2.13` to the Windows host is reachable

Run:

```bash
aw-hayabusa-from-windows --days-back 1 --mode incident --case-id 30
```

This performs:

- Windows EVTX export via WinRM
- fetch of the newest zip directly onto `192.0.2.13`
- `aw-hayabusa accept`
- `aw-hayabusa process-inbox`
- bounded case linkage via case API

If WinRM from the server to Windows is blocked by network policy, use the drop-zone workflow below instead.

## Drop-zone automation on 192.0.2.13

The server can auto-process packages dropped into:

- `/opt/activitywatch/aw-rus-ops/drop`

Installed units:

- `/etc/systemd/system/aw-hayabusa-drop.service`
- `/etc/systemd/system/aw-hayabusa-drop.path`

Behavior:

- any `*.zip` placed in `drop/` is automatically accepted and processed
- optional `*.caseid` sidecar with the same basename triggers automatic bounded case linkage
- processed `*.zip` is moved out of `drop/` into `report_dir/input-drop/` to avoid repeated re-trigger loops
- sidecars are archived into `report_dir/input-sidecars/`
- bad drop packages are rejected before `accept`, moved to
  `/opt/hayabusa/quarantine/drop/<timestamp>_<package>/`, and recorded with a
  `reason.json` file instead of blocking later packages
- bad or partially extracted incoming packages are moved to
  `/opt/hayabusa/quarantine/incoming/<timestamp>_<package>/`; `process-inbox`
  continues with the remaining queue and does not trip systemd start-limit only
  because of one poison archive

## Windows direct upload into the drop zone

Preferred production path when server-side WinRM is unavailable:

1. On Windows, use:

```powershell
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-upload-hayabusa-to-aw-server.ps1 -DaysBack 1 -CaseId 30
```

2. The script will:

- run `C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1`
- upload matching `.caseid` first when `-CaseId` is specified
- upload the newest zip to `/opt/activitywatch/aw-rus-ops/drop`
- let `aw-hayabusa-drop.path` process the package automatically on `192.0.2.13`

This path was validated live against case `30` after the `awops` SSH authorization was installed on `192.0.2.13`.

Server-side prerequisite for user `awops`:

```bash
printf '%s\n' 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILoFWQmgoUJj1P7mp1/fB5aBkI3fVgjPme9jmK8Gh9jr igor@snb-live' | sudo tee /var/lib/awops/.ssh/authorized_keys >/dev/null
sudo chown awops:awops /var/lib/awops/.ssh/authorized_keys
sudo chmod 600 /var/lib/awops/.ssh/authorized_keys
sudo chown awops:awops /opt/activitywatch/aw-rus-ops/drop
sudo chmod 0750 /opt/activitywatch/aw-rus-ops/drop
```

Production scheduled task on `SHARKON2025`:

- task name: `ActivityWatch Hayabusa Upload`
- action: `C:\ProgramData\AWatch-rus\export-upload-hayabusa-to-aw-server.ps1 -HoursBack 6 -Mode incident`
- principal: `Администратор`, `LogonType=Interactive`, `RunLevel=Highest`
- normal `LastTaskResult`: `0`

Do not switch this task back to `SYSTEM` on the current RDP host: Task Scheduler starts `powershell.exe` under `SYSTEM`, but the process exits with `0xC0000142` before the upload script starts.

Server-side processing accepts Windows zip packages with backslash path
separators and UTF-8 BOM in sidecar JSON. `aw-hayabusa-autoprocess` processes
the full incoming queue after accepting a drop package, so stale incoming files
from an earlier failed run are drained before the latest intake is recorded.

Poison-package handling is fail-closed:

- Rust `aw-hayabusa-autoprocess-rust` validates the zip and sidecars before
  calling `aw-hayabusa accept`.
- A corrupt/empty/unsafe drop package is quarantined with its `.meta.json`,
  `.caseid`, optional checksum sidecar and `reason.json`.
- `aw-hayabusa process-inbox` isolates a failed incoming package instead of
  aborting the whole batch.
- Operators replay only a fixed/re-exported package by moving it back to the
  drop zone or incoming queue. Do not edit quarantined evidence in place.

## Security Finding Inbox integration

`aw-hayabusa-autoprocess-rust` can publish a normalized suspicious-workstation
finding after a successful intake is written to `/opt/hayabusa/state/latest-intake.json`.

Default is disabled to keep forensic processing independent from ClickHouse:

```bash
AW_SECURITY_FINDING_INBOX_ENABLED=false
```

Enable after the ClickHouse schema and CLI are installed:

```bash
AW_SECURITY_FINDING_INBOX_ENABLED=true
AW_SECURITY_FINDING_INBOX_BIN=/usr/local/bin/security-finding-inbox
AW_SECURITY_FINDING_INBOX_MIN_SEVERITY=medium
AW_SECURITY_FINDING_INBOX_REQUIRED=false
```

With `AW_SECURITY_FINDING_INBOX_REQUIRED=false`, a temporary ClickHouse/inbox
failure is logged as warning and does not poison the Hayabusa backlog. Use
`true` only when the operator wants inbox publication failure to become an
operational failure for the drop service.
