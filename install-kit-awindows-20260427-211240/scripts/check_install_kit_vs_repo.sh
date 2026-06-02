#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KIT_DIR="install-kit-awindows-20260427-211240"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RUST_BIN="${CHECK_INSTALL_KIT_VS_REPO_RUST:-}"

rust_candidates=()
if [[ -n "$RUST_BIN" ]]; then
  rust_candidates+=("$RUST_BIN")
fi
rust_candidates+=(
  "$TARGET_ROOT/release/check-install-kit-vs-repo"
  "$ROOT_DIR/adk-rust/target/release/check-install-kit-vs-repo"
  "/usr/local/bin/check-install-kit-vs-repo"
)

for candidate in "${rust_candidates[@]}"; do
  if [[ -x "$candidate" ]]; then
    exec "$candidate" --root "$ROOT_DIR" --kit-dir "$KIT_DIR" "$@"
  fi
done

python3 - <<'PY'
from pathlib import Path
import hashlib
import sys

root=Path('.')
kit=Path('install-kit-awindows-20260427-211240')
if not kit.exists():
    raise SystemExit('Install kit directory not found')

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

all_compared=[]
mismatches=[]
missing_in_repo=[]
allowed_kit_only_prefixes=('server-configs-' ,)
allowed_kit_only_files={'README-INSTALL-KIT.txt'}

for kp in sorted(p for p in kit.rglob('*') if p.is_file() and p.name!='MANIFEST.txt'):
    rel=kp.relative_to(kit)
    rp=root/rel
    if not rp.exists():
        rel_str=str(rel)
        if rel_str in allowed_kit_only_files or rel_str.startswith(allowed_kit_only_prefixes):
            continue
        missing_in_repo.append(str(rel))
        continue
    all_compared.append(str(rel))
    if sha(kp)!=sha(rp):
        mismatches.append(str(rel))

ps_mismatches=[
    p for p in mismatches
    if p.startswith('windows/') and p.endswith(('.ps1', '.psm1', '.psd1'))
]

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
if missing_in_repo or mismatches:
    sys.exit(1)
PY
