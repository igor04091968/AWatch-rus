#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KIT_DIR="install-kit-awindows-20260427-211240"
MANIFEST="$KIT_DIR/MANIFEST.txt"
ZIP_ARCHIVE="install-kit-awindows-20260427-211240.zip"
TAR_ARCHIVE="install-kit-awindows-20260427-211240.tar.gz"

required_files=(
  "$MANIFEST"
  "$KIT_DIR/README-INSTALL-KIT.txt"
  "$KIT_DIR/windows/deploy-ensemble.ps1"
  "$KIT_DIR/windows/validate-deployment.ps1"
  "$KIT_DIR/ansible/deploy_aw_windows_phase2.yml"
  "$KIT_DIR/aw-server/install_aw_server.sh"
)

echo "[1/4] Required files presence"
for file in "${required_files[@]}"; do
  [[ -f "$file" ]] || { echo "Missing required file: $file"; exit 1; }
done

echo "[2/4] Manifest checksum verification"
sha256sum -c "$MANIFEST" >/dev/null

echo "[3/4] Manifest completeness"
python - <<'PY'
from pathlib import Path
import sys
kit=Path('install-kit-awindows-20260427-211240')
manifest=kit/'MANIFEST.txt'
listed=[]
for line in manifest.read_text().splitlines():
    line=line.strip()
    if not line:
        continue
    parts=line.split('  ',1)
    if len(parts)!=2:
        print(f'Invalid MANIFEST line: {line}')
        sys.exit(1)
    listed.append(parts[1])
listed_set=set(listed)
actual_set={str(p) for p in kit.rglob('*') if p.is_file() and p.name!='MANIFEST.txt'}
missing=sorted(listed_set-actual_set)
extra=sorted(actual_set-listed_set)
if missing or extra:
    print('Missing files listed in MANIFEST:', missing)
    print('Files not listed in MANIFEST:', extra)
    sys.exit(1)
print(f'MANIFEST complete: {len(actual_set)} files tracked')
PY

echo "[4/4] Archive composition check"
python - <<'PY'
from pathlib import Path
import tarfile, zipfile, sys
kit_prefix='install-kit-awindows-20260427-211240/'
zip_path=Path('install-kit-awindows-20260427-211240.zip')
tar_path=Path('install-kit-awindows-20260427-211240.tar.gz')
if not zip_path.exists() or not tar_path.exists():
    print('Archives not found')
    sys.exit(1)
with zipfile.ZipFile(zip_path) as z:
    zip_files=sorted(i for i in z.namelist() if not i.endswith('/'))
with tarfile.open(tar_path, 'r:gz') as t:
    tar_files=sorted(m.name for m in t.getmembers() if m.isfile())
if zip_files != tar_files:
    print('ZIP and TAR contents differ')
    sys.exit(1)
if not all(f.startswith(kit_prefix) for f in zip_files):
    print('Unexpected archive prefix layout')
    sys.exit(1)
print(f'Archives match: {len(zip_files)} files')
PY

echo "validate_install_kit: OK"
