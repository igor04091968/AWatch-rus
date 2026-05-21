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

## Install-time parameters (Standalone agent mode)

The installer wizard asks only for:

- `ServerHost` / `ServerPort` (defaults to `aw-server:5600`)

All other values are taken from defaults embedded in installer scripts.

## Runtime mode

- Installer runs `windows\install-standalone-service.ps1`.
- A Windows service `AWatchRusStandaloneAgent` is created with auto-start and restart-on-failure.
- Service wrapper (`windows\aw-standalone-service.ps1`) keeps DLP collectors running:
  - `browser-domains-native-collector.ps1`
  - `dlp-endpoint-signals-collector.ps1`
  - `file-operations-collector.ps1`
  - `email-outbound-collector.ps1` (if present)
  - `worktime-session-collector.ps1` (if present)
