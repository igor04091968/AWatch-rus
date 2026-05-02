#!/usr/bin/env bash
set -euo pipefail

KIT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
WINEPREFIX_DEFAULT="${HOME}/.wine-aw-inno"
WINEPREFIX="${WINEPREFIX:-$WINEPREFIX_DEFAULT}"

IS_EXE="${IS_EXE:-/tmp/innosetup.exe}"
ISCC_WIN='C:\InnoSetup\ISCC.exe'

cd "$KIT_DIR"

mkdir -p payload

ZIP_NAME="activitywatch-v0.13.2-windows-x86_64.zip"

# If someone dropped the ZIP in the kit root, stage it into payload/.
if [[ -f "$ZIP_NAME" && ! -f "payload/$ZIP_NAME" ]]; then
  cp -f "$ZIP_NAME" "payload/$ZIP_NAME"
fi

export WINEPREFIX
export WINEDEBUG="${WINEDEBUG:--all}"

if [[ ! -f "${WINEPREFIX}/drive_c/InnoSetup/ISCC.exe" ]]; then
  mkdir -p "$WINEPREFIX"
  if [[ ! -f "$IS_EXE" ]]; then
    curl -fsSL -o "$IS_EXE" https://jrsoftware.org/download.php/is.exe
  fi
  wineboot -u
  wine "$IS_EXE" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /DIR="C:\InnoSetup"
fi

rm -f AWatch-rus-InstallKit.exe
wine "$ISCC_WIN" "AWatch-rus-InnoSetup.iss"

ls -la AWatch-rus-InstallKit.exe
sha256sum AWatch-rus-InstallKit.exe
