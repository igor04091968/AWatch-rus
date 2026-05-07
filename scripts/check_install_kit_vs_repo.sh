#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KIT_DIR="install-kit-awindows-20260427-211240"

python - <<'PY'
from pathlib import Path
import hashlib

root=Path('.')
kit=Path('install-kit-awindows-20260427-211240')
if not kit.exists():
    raise SystemExit('Install kit directory not found')

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

all_compared=[]
mismatches=[]
missing_in_repo=[]

for kp in sorted(p for p in kit.rglob('*') if p.is_file() and p.name!='MANIFEST.txt'):
    rel=kp.relative_to(kit)
    rp=root/rel
    if not rp.exists():
        missing_in_repo.append(str(rel))
        continue
    all_compared.append(str(rel))
    if sha(kp)!=sha(rp):
        mismatches.append(str(rel))

ps_mismatches=[p for p in mismatches if p.startswith('windows/') and p.endswith('.ps1') or p.endswith('.psm1') or p.endswith('.psd1')]

print(f'Compared files: {len(all_compared)}')
print(f'Missing in repo: {len(missing_in_repo)}')
print(f'Mismatched content: {len(mismatches)}')
if missing_in_repo:
    print('--- Missing in repo ---')
    for p in missing_in_repo:
        print(p)
if mismatches:
    print('--- Mismatches ---')
    for p in mismatches:
        print(p)
print(f'PowerShell mismatches: {len(ps_mismatches)}')
if ps_mismatches:
    print('--- PowerShell mismatches ---')
    for p in ps_mismatches:
        print(p)
PY
