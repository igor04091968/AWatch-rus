# AWatch-rus Inno Setup installer skeleton

This directory contains a ready-to-build scaffold for a classic Setup.exe installer.

## Files
- `AWatch-rus-Setup.iss` — Inno Setup script.
- `Invoke-AWatchRusInstall.ps1` — wrapper that invokes `deploy-ensemble.ps1` with installer parameters.

## Build
1. Install Inno Setup 6 on Windows build host.
2. Open `AWatch-rus-Setup.iss` in Inno Setup Compiler.
3. Build to produce `output/AWatch-rus-Setup.exe`.

## Behavior
- Requires Administrator privileges.
- Copies toolkit scripts into `{app}\tools`.
- Collects server/domain/users parameters via wizard page.
- Runs `Invoke-AWatchRusInstall.ps1` -> `deploy-ensemble.ps1`.
- Runs validation at the end.
- Uninstall step removes `ActivityWatch` scheduled tasks.

## Notes
- Default publisher URL is a placeholder (`https://example.local/awatch-rus`), replace it.
- For silent enterprise rollout, extend `[Run]` and parse `/SERVERHOST=...` style custom switches if needed.
