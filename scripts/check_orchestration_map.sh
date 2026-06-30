#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOC="$ROOT/docs/ORCHESTRATION_MAP_RU.md"
failures=()

fail() {
  failures+=("$1")
}

require_file() {
  local path="$1"
  if [[ ! -s "$ROOT/$path" ]]; then
    fail "missing_or_empty:$path"
  fi
}

require_marker() {
  local marker="$1"
  local path="$2"
  if ! grep -Fq "$marker" "$ROOT/$path"; then
    fail "missing_marker:$path:$marker"
  fi
}

require_absent() {
  local marker="$1"
  local path="$2"
  if grep -Fq "$marker" "$ROOT/$path"; then
    fail "forbidden_marker:$path:$marker"
  fi
}

require_file "docs/ORCHESTRATION_MAP_RU.md"
require_file "docs/MODULE_ARCHITECTURE_GRAPH_RU.md"
require_file "ansible/README.md"
require_file "README.md"

entrypoints=(
  "ansible/install_full_stack.yml"
  "ansible/deploy_aw_server.yml"
  "ansible/deploy_aw_windows.yml"
  "ansible/post_validate_aw_windows.yml"
  "ansible/deploy_detmir_portal.yml"
  "ansible/deploy_proxmox_web_gateway.yml"
  "ansible/deploy_grafana_dashboards.yml"
  "ansible/deploy_grafana_check.yml"
  "ansible/deploy_file_1c_analytics.yml"
  "ansible/deploy_file_1c_windows_telemetry.yml"
  "ansible/deploy_dlp_evidence_sync.yml"
  "ansible/deploy_aw_pfsense_poller.yml"
  "scripts/run_awatch_contour_check.sh"
  "scripts/detmir-support-daily.sh"
  "scripts/check_orchestration_map.sh"
  "ops/systemd/awatch-contour-daily-check.timer"
  "ops/systemd/awatch-contour-weekly-check.timer"
)

for path in "${entrypoints[@]}"; do
  require_file "$path"
  require_marker "$path" "docs/ORCHESTRATION_MAP_RU.md"
done

doc_markers=(
  "GitHub/Gitea"
  "DLP runtime"
  "Hayabusa"
  "Velociraptor"
  "Security findings"
  "approval"
  "SHARKON2025"
  "logical host id"
  "quality-gate.sh"
  "public mirror validation"
  "российский build-runner"
)

for marker in "${doc_markers[@]}"; do
  require_marker "$marker" "docs/ORCHESTRATION_MAP_RU.md"
done

require_marker "docs/ORCHESTRATION_MAP_RU.md" "docs/MODULE_ARCHITECTURE_GRAPH_RU.md"
require_marker "docs/ORCHESTRATION_MAP_RU.md" "README.md"
require_marker "docs/ORCHESTRATION_MAP_RU.md" "ansible/README.md"

require_absent "FSTEC certified" "docs/ORCHESTRATION_MAP_RU.md"
require_absent "ФСТЭК сертифицирован" "docs/ORCHESTRATION_MAP_RU.md"
require_absent "automatic remediation" "docs/ORCHESTRATION_MAP_RU.md"
require_absent "registry submission completed" "docs/ORCHESTRATION_MAP_RU.md"

if (( ${#failures[@]} > 0 )); then
  printf 'orchestration_map_check=fail\n' >&2
  printf '%s\n' "${failures[@]}" >&2
  exit 1
fi

printf 'orchestration_map_check=ok\n'
