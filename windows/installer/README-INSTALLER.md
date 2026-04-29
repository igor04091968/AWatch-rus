# AWatch-rus Inno Setup installer skeleton

This directory contains a ready-to-build scaffold for a classic Setup.exe installer.

## Files
- `AWatch-rus-Setup.iss` — Inno Setup script.
- `Invoke-AWatchRusInstall.ps1` — wrapper that invokes `deploy-ensemble.ps1` with installer parameters.
- `build-installer.ps1` — one-click local build script for Inno Setup.

## Local build (one-click)
```powershell
cd .\windows\installer
.\build-installer.ps1
```

Optional custom path to compiler:
```powershell
.\build-installer.ps1 -IsccPath 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
```

## Silent deployment parameters (SCCM/Intune/GPO)
Installer supports command-line parameters:
- `/SERVERHOST=aw.example.local`
- `/SERVERPORT=5600`
- `/DOMAIN=CONTOSO`
- `/USERS=user1,user2`
- `/INSTALLROOT="C:\Program Files\ActivityWatch"`
- `/STATEROOT="C:\ProgramData\ActivityWatch"`
- `/AFKENABLED=true|false`
- `/WINDOWENABLED=true|false`

Example:
```powershell
AWatch-rus-Setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART `
  /SERVERHOST=aw.example.local /SERVERPORT=5600 /DOMAIN=CONTOSO `
  /USERS=user1,user2 /AFKENABLED=true /WINDOWENABLED=true
```

## CI build and optional code signing
A GitHub Actions workflow builds the installer on `windows-latest` and uploads `Setup.exe` as an artifact.

Optional production signing is enabled via `SignTool=byparam ...` in `.iss` and expects `SIGNTOOL_CMD`.
Set repository secret `SIGNTOOL_CMD` with your full sign command template.

## Behavior
- Requires Administrator privileges.
- Copies toolkit scripts into `{app}\tools`.
- Collects server/domain/users parameters via wizard page (or command-line silent params).
- Runs `Invoke-AWatchRusInstall.ps1` -> `deploy-ensemble.ps1`.
- Runs validation at the end.
- Uninstall step removes `ActivityWatch` scheduled tasks.
