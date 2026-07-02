#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v node >/dev/null 2>&1; then
  echo "run_full_validation=fail node_not_found" >&2
  exit 127
fi

run_preflight_self_test=1
for arg in "$@"; do
  if [[ "$arg" == "--self-test" || "$arg" == "--help" || "$arg" == "-h" ]]; then
    run_preflight_self_test=0
  fi
done

if [[ "${AW_VALIDATION_SKIP_SELF_TEST:-0}" != "1" && "$run_preflight_self_test" == "1" ]]; then
  node scripts/full-validation-orchestrator.mjs --self-test
fi

exec node scripts/full-validation-orchestrator.mjs "$@"
