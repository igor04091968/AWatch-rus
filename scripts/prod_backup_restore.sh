#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ -f "${ROOT_DIR}/private-config/runtime.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/private-config/runtime.env"
  set +a
fi

for candidate in \
  "${PROD_BACKUP_RESTORE_RUST:-}" \
  "${CARGO_TARGET_DIR:-}/release/prod-backup-restore" \
  "$ROOT_DIR/adk-rust/target/release/prod-backup-restore" \
  "/usr/local/bin/prod-backup-restore"; do
  if [[ -n "$candidate" && -x "$candidate" ]]; then
    exec "$candidate" --root "$ROOT_DIR" "$@"
  fi
done

cat >&2 <<EOF
prod_backup_restore.sh now requires the Rust planner/checker.
Build it first:
  cd "$ROOT_DIR/adk-rust" && cargo build --release -p prod-backup-restore
EOF
exit 127
