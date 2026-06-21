#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REGISTRY_DIR="$ROOT/docs/registry"
MANIFEST="$REGISTRY_DIR/registry-evidence-manifest.json"

failures=()

fail() {
  failures+=("$1")
}

require_file() {
  local file="$1"
  if [[ ! -s "$ROOT/$file" ]]; then
    fail "missing_or_empty:$file"
  fi
}

require_grep() {
  local pattern="$1"
  local file="$2"
  local name="$3"
  if ! grep -Eiq "$pattern" "$ROOT/$file"; then
    fail "missing_marker:$name:$file"
  fi
}

required_files=(
  "docs/registry/REGISTER_RU_SOFTWARE_READINESS_RU.md"
  "docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md"
  "docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md"
  "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md"
  "docs/registry/RELEASE_EVIDENCE_MANIFEST_RU.md"
  "docs/registry/INSTALLATION_AND_TEST_INSTANCE_RU.md"
  "docs/registry/LIFECYCLE_AND_SUPPORT_RU.md"
  "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md"
  "docs/registry/registry-evidence-manifest.json"
  "README.md"
)

for file in "${required_files[@]}"; do
  require_file "$file"
done

if [[ -s "$MANIFEST" ]]; then
  if command -v python3 >/dev/null 2>&1; then
    python3 -m json.tool "$MANIFEST" >/dev/null || fail "invalid_json:docs/registry/registry-evidence-manifest.json"
    python3 - "$MANIFEST" <<'PY' || fail "manifest_required_fields"
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

expected = {
    "primary_source_repository": "https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus",
    "primary_git_platform": "self-hosted Gitea",
    "primary_git_provider": "REG.RU VPS / cloud server",
    "github_role": "public_mirror_only",
}
for key, value in expected.items():
    if data.get(key) != value:
        raise SystemExit(f"{key} mismatch")

backup = data.get("backup") or {}
backup_expected = {
    "enabled": True,
    "tool": "gitea dump",
    "path": "/var/backups/gitea",
    "checksum": "sha256",
    "retention_days": 14,
    "systemd_timer": "awatch-gitea-backup.timer",
    "restore_tested": False,
}
for key, value in backup_expected.items():
    if backup.get(key) != value:
        raise SystemExit(f"backup.{key} mismatch")
PY
  elif command -v node >/dev/null 2>&1; then
    node -e 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))' "$MANIFEST" \
      || fail "invalid_json:docs/registry/registry-evidence-manifest.json"
  else
    fail "json_validator_missing:python3_or_node_required"
  fi
fi

require_grep "git\\.iri1968\\.dpdns\\.org" "docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md" "gitea_domain_source_infra"
require_grep "git\\.iri1968\\.dpdns\\.org" "docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md" "gitea_domain_git_runbook"
require_grep "git\\.iri1968\\.dpdns\\.org" "README.md" "gitea_domain_readme"
require_grep "GitHub[[:space:]]*=[[:space:]]*public mirror only|public mirror only" "docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md" "github_public_mirror_source_infra"
require_grep "GitHub[[:space:]]*=[[:space:]]*public mirror only|public mirror only" "docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md" "github_public_mirror_git_runbook"
require_grep "public mirror" "README.md" "github_public_mirror_readme"
require_grep "GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU\\.md|Restore outline|Post-restore checks" "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md" "backup_restore_runbook"
require_grep "awatch-gitea-backup\\.timer" "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md" "backup_timer"
require_grep "sha256|SHA256" "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md" "backup_sha256"

scan_files=(
  "$ROOT/README.md"
  "$REGISTRY_DIR"/*.md
)

if grep -RInEi "(ФСТЭК|ФСБ).{0,80}(сертифицирован|сертификация|сертификат).{0,80}(есть|получен|имеется|подтвержден)" "${scan_files[@]}" >/tmp/registry_forbidden_fstec_fsb.$$ 2>/dev/null; then
  fail "forbidden_claim_fstec_fsb_certification:$(cat /tmp/registry_forbidden_fstec_fsb.$$)"
fi
rm -f /tmp/registry_forbidden_fstec_fsb.$$

if grep -RInEi "(заменяет|replacement for|replaces).{0,80}(SIEM|DLP)|((SIEM|DLP).{0,80}(replacement|заменяет))" "${scan_files[@]}" \
  | grep -Eiv "(не |not |does not|не является|не подменяет|не заявляет|forbidden|not_made)" \
  >/tmp/registry_forbidden_replacement.$$ 2>/dev/null; then
  fail "forbidden_claim_siem_dlp_replacement:$(cat /tmp/registry_forbidden_replacement.$$)"
fi
rm -f /tmp/registry_forbidden_replacement.$$

if grep -RInEi "(ML/LLM-based detection|LLM-based detection|ML-based detection|automatic remediation)" "${scan_files[@]}" \
  | grep -Eiv "(forbidden|not_made|не заявляет|не фиксируется|не используется)" \
  >/tmp/registry_forbidden_ai_auto.$$ 2>/dev/null; then
  fail "forbidden_claim_ai_or_automatic_remediation:$(cat /tmp/registry_forbidden_ai_auto.$$)"
fi
rm -f /tmp/registry_forbidden_ai_auto.$$

if ((${#failures[@]} > 0)); then
  printf 'registry_readiness_check=fail\n'
  for failure in "${failures[@]}"; do
    printf '%s\n' "$failure"
  done
  exit 2
fi

printf 'registry_readiness_check=ok\n'
printf 'checked_files=%d\n' "${#required_files[@]}"
