# Build (Inno Setup)

## Prerequisites

- Windows machine with Inno Setup installed (`ISCC.exe` available).

## Offline vs Online payload

- **Offline (recommended for closed networks)**: put `activitywatch-v0.13.2-windows-x86_64.zip` into `payload\`.
- **Online**: leave `payload\` empty; the deploy script will download the ZIP from GitHub Releases.

## Compile

From this folder:

```bat
iscc AWatch-rus-InnoSetup.iss
```

The resulting installer `AWatch-rus-InstallKit.exe` is written to the same directory (by `OutputDir=.`).

## Compile from Linux (Wine)

```sh
./build_with_wine.sh
```

## Install-time parameters

The installer wizard asks for:

- `ServerHost` / `ServerPort` (defaults to our AW server `10.10.10.13:5600`)
- `Users` (CSV)
- Whether to use offline payload (auto-enabled when the ZIP exists at compile time)
- Whether to validate after deploy (`-ValidateAfterDeploy`, report written to `C:\ProgramData\AWatch-rus\ensemble-report-*.json`)
