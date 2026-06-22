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

if [[ ! -d "$REGISTRY_DIR" ]]; then
  fail "missing_directory:docs/registry"
fi

required_files=(
  "docs/registry/REGISTER_RU_SOFTWARE_READINESS_RU.md"
  "docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md"
  "docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md"
  "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md"
  "docs/registry/WIKI_AND_DOCUMENTATION_POLICY_RU.md"
  "docs/registry/RU_BUILD_RUNNER_READINESS_RU.md"
  "docs/registry/BUILD_RUNNER_SETUP_RUNBOOK_RU.md"
  "docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md"
  "docs/registry/RELEASE_ARTIFACTS_STORAGE_RU.md"
  "docs/registry/RELEASE_EVIDENCE_MANIFEST_RU.md"
  "docs/registry/INSTALLATION_AND_TEST_INSTANCE_RU.md"
  "docs/registry/LIFECYCLE_AND_SUPPORT_RU.md"
  "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md"
  "docs/registry/registry-evidence-manifest.json"
  "docs/PROJECT_STATUS_RU.md"
  "docs/QUALITY_STATUS_RU.md"
  "docs/SECURITY_SCANNING_POLICY_RU.md"
  "docs/REVIEW_CHECKLIST_RU.md"
  "docs/RESIDUAL_RISKS_RU.md"
  "docs/PUBLIC_ISSUES_PLAN_RU.md"
  "docs/BRANCH_PROTECTION_POLICY_RU.md"
  "scripts/build_release_evidence.sh"
  "scripts/check_release_evidence.sh"
  "scripts/public_secret_pattern_check.py"
  ".github/CODEOWNERS"
  ".github/workflows/ci.yml"
  ".github/workflows/security.yml"
  ".github/workflows/coverage.yml"
  ".github/ISSUE_TEMPLATE/bug_report.yml"
  ".github/ISSUE_TEMPLATE/feature_request.yml"
  ".github/ISSUE_TEMPLATE/registry_readiness_task.yml"
  ".github/ISSUE_TEMPLATE/security_hardening_task.yml"
  ".github/pull_request_template.md"
  "SECURITY.md"
  "CONTRIBUTING.md"
  "ROADMAP.md"
  "deny.toml"
  "README.md"
)

for file in "${required_files[@]}"; do
  require_file "$file"
done

if [[ -s "$MANIFEST" ]]; then
  if command -v jq >/dev/null 2>&1; then
    jq -e . "$MANIFEST" >/dev/null || fail "invalid_json:docs/registry/registry-evidence-manifest.json"
    jq -e '
      .primary_source_repository == "https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus"
      and .primary_git_clone_url_https == "https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus.git"
      and .primary_git_platform == "self-hosted Gitea"
      and .primary_git_provider == "REG.RU VPS / cloud server"
      and .github_role == "public_mirror_only"
      and .wiki_policy.github_wiki_detected == false
      and .wiki_policy.gitea_builtin_wiki == "navigation_only"
      and .wiki_policy.authoritative_docs_path == "docs/registry"
      and .backup.enabled == true
      and .backup.tool == "gitea dump"
      and .backup.path == "/var/backups/gitea"
      and .backup.checksum == "sha256"
      and .backup.retention_days == 14
      and .backup.systemd_timer == "awatch-gitea-backup.timer"
      and .backup.restore_tested == false
      and .build_runner.status != "production_ready"
      and .build_runner.status == "planned"
      and .build_runner.target_hostname == "awatch-build-01"
      and .build_runner.separate_from_git_server == true
      and (.build_runner.required_checks | index("release_evidence_check") != null)
      and .public_engineering_transparency.github_actions_ci == true
      and .public_engineering_transparency.github_actions_ci_status == "passed"
      and .public_engineering_transparency.coverage_baseline == true
      and .public_engineering_transparency.coverage_workflow_status == "passed"
      and .public_engineering_transparency.security_scanning == true
      and .public_engineering_transparency.security_workflow_status == "passed"
      and .public_engineering_transparency.secret_scan_status == "passed"
      and .public_engineering_transparency.issue_templates == true
      and .public_engineering_transparency.public_roadmap == true
      and .public_engineering_transparency.github_role == "public_mirror_validation_only"
      and .public_engineering_transparency.registry_release_build == "requires_russian_build_runner"
    ' "$MANIFEST" >/dev/null || fail "manifest_required_fields"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -m json.tool "$MANIFEST" >/dev/null || fail "invalid_json:docs/registry/registry-evidence-manifest.json"
    python3 - "$MANIFEST" <<'PY' || fail "manifest_required_fields"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

expected = {
    "primary_source_repository": "https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus",
    "primary_git_clone_url_https": "https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus.git",
    "primary_git_platform": "self-hosted Gitea",
    "primary_git_provider": "REG.RU VPS / cloud server",
    "github_role": "public_mirror_only",
}
for key, value in expected.items():
    if data.get(key) != value:
        raise SystemExit(f"{key} mismatch")

wiki_policy = data.get("wiki_policy") or {}
wiki_expected = {
    "github_wiki_detected": False,
    "gitea_builtin_wiki": "navigation_only",
    "authoritative_docs_path": "docs/registry",
}
for key, value in wiki_expected.items():
    if wiki_policy.get(key) != value:
        raise SystemExit(f"wiki_policy.{key} mismatch")

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

build_runner = data.get("build_runner") or {}
if build_runner.get("status") == "production_ready":
    raise SystemExit("build_runner.status must not be production_ready")
if build_runner.get("status") != "planned":
    raise SystemExit("build_runner.status mismatch")
if build_runner.get("target_hostname") != "awatch-build-01":
    raise SystemExit("build_runner.target_hostname mismatch")
if build_runner.get("separate_from_git_server") is not True:
    raise SystemExit("build_runner.separate_from_git_server mismatch")
if "release_evidence_check" not in build_runner.get("required_checks", []):
    raise SystemExit("build_runner.required_checks missing release_evidence_check")

public = data.get("public_engineering_transparency") or {}
public_expected = {
    "github_actions_ci": True,
    "github_actions_ci_status": "passed",
    "coverage_baseline": True,
    "coverage_workflow_status": "passed",
    "security_scanning": True,
    "security_workflow_status": "passed",
    "secret_scan_status": "passed",
    "issue_templates": True,
    "public_roadmap": True,
    "github_role": "public_mirror_validation_only",
    "registry_release_build": "requires_russian_build_runner",
}
for key, value in public_expected.items():
    if public.get(key) != value:
        raise SystemExit(f"public_engineering_transparency.{key} mismatch")
PY
  else
    fail "json_validator_missing:jq_or_python3_required"
  fi
fi

require_grep "git\\.iri1968\\.dpdns\\.org" "docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md" "gitea_domain_source_infra"
require_grep "git\\.iri1968\\.dpdns\\.org" "docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md" "gitea_domain_git_runbook"
require_grep "git\\.iri1968\\.dpdns\\.org" "docs/registry/WIKI_AND_DOCUMENTATION_POLICY_RU.md" "gitea_domain_wiki_policy"
require_grep "git\\.iri1968\\.dpdns\\.org" "docs/registry/RU_BUILD_RUNNER_READINESS_RU.md" "gitea_domain_build_runner"
require_grep "git\\.iri1968\\.dpdns\\.org" "README.md" "gitea_domain_readme"
require_grep "awatch-build-01" "docs/registry/RU_BUILD_RUNNER_READINESS_RU.md" "awatch_build_runner_readiness"
require_grep "awatch-build-01" "docs/registry/BUILD_RUNNER_SETUP_RUNBOOK_RU.md" "awatch_build_runner_setup"
require_grep "awatch-build-01" "docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md" "awatch_build_runner_release_runbook"
require_grep "GitHub[[:space:]]*=[[:space:]]*public mirror only|public mirror only" "docs/registry/SOURCE_CODE_AND_BUILD_INFRASTRUCTURE_RU.md" "github_public_mirror_source_infra"
require_grep "GitHub[[:space:]]*=[[:space:]]*public mirror only|public mirror only" "docs/registry/GIT_RU_MIRRORING_RUNBOOK_RU.md" "github_public_mirror_git_runbook"
require_grep "GitHub.*публичн|public mirror" "docs/registry/WIKI_AND_DOCUMENTATION_POLICY_RU.md" "github_public_mirror_wiki_policy"
require_grep "public mirror|публичн.*зеркал" "README.md" "github_public_mirror_readme"
require_grep "PROJECT_STATUS_RU\\.md" "README.md" "readme_project_status_link"
require_grep "4970d31" "docs/PROJECT_STATUS_RU.md" "project_status_baseline_commit"
require_grep "git\\.iri1968\\.dpdns\\.org/awatch-rus/AWatch-rus" "docs/PROJECT_STATUS_RU.md" "project_status_primary_git"
require_grep "public mirror / public validation only" "docs/PROJECT_STATUS_RU.md" "project_status_github_role"
require_grep "Public CI status:[[:space:]]*passed" "docs/PROJECT_STATUS_RU.md" "project_status_public_ci_passed"
require_grep "Public coverage workflow:[[:space:]]*passed" "docs/PROJECT_STATUS_RU.md" "project_status_public_coverage_passed"
require_grep "Public security workflow:[[:space:]]*passed" "docs/PROJECT_STATUS_RU.md" "project_status_public_security_passed"
require_grep "Secret scan:[[:space:]]*hardened and passed" "docs/PROJECT_STATUS_RU.md" "project_status_secret_scan_passed"
require_grep "Public validation passed.*not.*registry release evidence|not registry release evidence" "docs/PROJECT_STATUS_RU.md" "project_status_public_validation_not_release_evidence"
require_grep "Russian build-runner.*required|requires_russian_build_runner" "docs/PROJECT_STATUS_RU.md" "project_status_russian_runner_required"
require_grep "docs/registry" "docs/PROJECT_STATUS_RU.md" "project_status_registry_docs"
require_grep "RESIDUAL_RISKS_RU\\.md" "docs/PROJECT_STATUS_RU.md" "project_status_residual_risks_link"
require_grep "PUBLIC_ISSUES_PLAN_RU\\.md" "docs/PROJECT_STATUS_RU.md" "project_status_public_issues_plan_link"
require_grep "GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU\\.md|Restore outline|Post-restore checks" "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md" "backup_restore_runbook"
require_grep "awatch-gitea-backup\\.timer" "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md" "backup_timer"
require_grep "sha256|SHA256" "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md" "backup_sha256"
require_grep "restore_tested=false|restore_tested..false|\"restore_tested\"[[:space:]]*:[[:space:]]*false" "docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md" "restore_tested_false_runbook"
require_grep "\"restore_tested\"[[:space:]]*:[[:space:]]*false" "docs/registry/registry-evidence-manifest.json" "restore_tested_false_manifest"
require_grep "docs/registry" "docs/registry/WIKI_AND_DOCUMENTATION_POLICY_RU.md" "authoritative_docs_path_wiki_policy"
require_grep "release_evidence_check" "docs/registry/registry-evidence-manifest.json" "release_evidence_check_manifest"
require_grep "Public engineering transparency" "README.md" "readme_public_engineering_transparency"
require_grep "SECURITY_SCANNING_POLICY_RU\\.md" "README.md" "readme_security_scanning_policy"
require_grep "GitHub Actions is public mirror validation only|public mirror validation only" "docs/registry/RU_BUILD_RUNNER_READINESS_RU.md" "github_actions_not_registry_build_runner"
require_grep "GitHub Actions is public mirror validation only|public mirror validation only" "docs/registry/RELEASE_EVIDENCE_RUNBOOK_RU.md" "github_actions_not_registry_release_runbook"
require_grep "Public CI is not registry release evidence|not registry release evidence" "docs/QUALITY_STATUS_RU.md" "quality_public_ci_not_registry_evidence"
require_grep "First public validation passed|public validation passed" "docs/QUALITY_STATUS_RU.md" "quality_first_public_validation_passed"
require_grep "Coverage.*passed" "docs/QUALITY_STATUS_RU.md" "quality_coverage_workflow_passed"
require_grep "Security.*passed" "docs/QUALITY_STATUS_RU.md" "quality_security_workflow_passed"
require_grep "Coverage threshold is not enforced yet" "docs/QUALITY_STATUS_RU.md" "quality_no_coverage_threshold"
require_grep "Russian build-runner.*required" "docs/QUALITY_STATUS_RU.md" "quality_russian_runner_required"
require_grep "requires_russian_build_runner|public_mirror_validation_only" "docs/registry/registry-evidence-manifest.json" "public_engineering_manifest"
require_grep "\"github_actions_ci_status\"[[:space:]]*:[[:space:]]*\"passed\"" "docs/registry/registry-evidence-manifest.json" "manifest_github_actions_ci_passed"
require_grep "\"coverage_workflow_status\"[[:space:]]*:[[:space:]]*\"passed\"" "docs/registry/registry-evidence-manifest.json" "manifest_coverage_passed"
require_grep "\"security_workflow_status\"[[:space:]]*:[[:space:]]*\"passed\"" "docs/registry/registry-evidence-manifest.json" "manifest_security_passed"
require_grep "\"secret_scan_status\"[[:space:]]*:[[:space:]]*\"passed\"" "docs/registry/registry-evidence-manifest.json" "manifest_secret_scan_passed"
require_grep "public GitHub Actions validation passed" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_public_validation_passed"
require_grep "No business logic changes" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_no_business_logic_changes"
require_grep "RESIDUAL_RISKS_RU\\.md" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_residual_risks"
require_grep "PUBLIC_ISSUES_PLAN_RU\\.md" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_public_issues_plan"
require_grep "Restore test is not claimed as completed" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_restore_not_completed"
require_grep "Russian build-runner is not claimed as ready" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_build_runner_not_ready"
require_grep "First release evidence build is not claimed as completed" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_release_evidence_not_completed"
require_grep "Legal rightsholder package remains pending" "docs/registry/REGISTRY_READINESS_CHANGELOG_RU.md" "changelog_legal_package_pending"
require_grep "RESIDUAL_RISKS_RU\\.md" "README.md" "readme_residual_risks_link"
require_grep "PUBLIC_ISSUES_PLAN_RU\\.md" "README.md" "readme_public_issues_plan_link"
require_grep "Engineering governance and residual risks" "README.md" "readme_engineering_governance_section"
require_grep "REVIEW_CHECKLIST_RU\\.md" "README.md" "readme_review_checklist_link"
require_grep "BRANCH_PROTECTION_POLICY_RU\\.md" "README.md" "readme_branch_protection_policy_link"
require_grep "CODEOWNERS" "README.md" "readme_codeowners"
require_grep "\\* @igor04091968" ".github/CODEOWNERS" "codeowners_default_owner"
require_grep "/adk-rust/" ".github/CODEOWNERS" "codeowners_rust_workspace"
require_grep "/scripts/" ".github/CODEOWNERS" "codeowners_scripts"
require_grep "/docs/registry/" ".github/CODEOWNERS" "codeowners_registry_docs"
require_grep "/\\.github/workflows/" ".github/CODEOWNERS" "codeowners_workflows"
require_grep "/ansible/" ".github/CODEOWNERS" "codeowners_ansible"
require_grep "SECURITY\\.md" ".github/CODEOWNERS" "codeowners_security_docs"
require_grep "не публиковать секреты|No secrets" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_no_secrets"
require_grep "персональных данных|personal data" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_no_pii"
require_grep "реальных IP|hostname|customer infrastructure identifiers" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_no_customer_infra"
require_grep "GitHub Actions.*public mirror validation" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_github_public_only"
require_grep "Release evidence must be produced on the Russian build-runner" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_russian_runner"
require_grep "Do not claim FSTEC/FSB certification" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_no_fstec_fsb"
require_grep "Do not claim SIEM/DLP replacement" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_no_siem_dlp"
require_grep "restore_tested=false|restore_tested.*false" "docs/REVIEW_CHECKLIST_RU.md" "review_checklist_restore_false"
require_grep "Require pull request before merge" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_require_pr"
require_grep "Require status checks" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_status_checks"
require_grep 'Require `CI` workflow' "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_ci"
require_grep 'Require `Security` workflow' "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_security"
require_grep 'Require `Coverage` workflow' "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_coverage"
require_grep "no coverage threshold" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_no_coverage_threshold"
require_grep "Restrict force push" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_force_push"
require_grep "Require conversation resolution" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_conversation_resolution"
require_grep "Require linear history" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_linear_history"
require_grep "emergency-only" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_admin_bypass"
require_grep "recommended policy|advisory branch protection policy" "docs/BRANCH_PROTECTION_POLICY_RU.md" "branch_policy_advisory_not_enabled"
require_grep "Один основной разработчик" "docs/RESIDUAL_RISKS_RU.md" "risk_single_developer"
require_grep "Нет внешнего visible peer review" "docs/RESIDUAL_RISKS_RU.md" "risk_peer_review"
require_grep "Низкая публичная активность issue tracker" "docs/RESIDUAL_RISKS_RU.md" "risk_issue_tracker_activity"
require_grep "Низкая community adoption" "docs/RESIDUAL_RISKS_RU.md" "risk_community_adoption"
require_grep "Gitea restore test еще не выполнен" "docs/RESIDUAL_RISKS_RU.md" "risk_restore_test_pending"
require_grep "restore_tested.*false" "docs/RESIDUAL_RISKS_RU.md" "risk_restore_tested_false"
require_grep "Российский build-runner пока planned" "docs/RESIDUAL_RISKS_RU.md" "risk_build_runner_planned"
require_grep "awatch-build-01.*planned|planned, not ready" "docs/RESIDUAL_RISKS_RU.md" "risk_awatch_build_01_not_ready"
require_grep "Первый настоящий release evidence build pending" "docs/RESIDUAL_RISKS_RU.md" "risk_release_evidence_pending"
require_grep "Юридический пакет правообладателя pending" "docs/RESIDUAL_RISKS_RU.md" "risk_rightsholder_pending"
require_grep "Почему не блокирует pilot/readiness stage" "docs/RESIDUAL_RISKS_RU.md" "risk_non_blocking_reason"
require_grep "Уже снижающие evidence/documents/CI" "docs/RESIDUAL_RISKS_RU.md" "risk_existing_evidence"
require_grep "\\[registry\\] Perform Gitea backup restore test" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_restore_test"
require_grep "\\[registry\\] Prepare temporary Russian build-runner awatch-build-01" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_build_runner"
require_grep "\\[release\\] Produce first release evidence package" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_release_evidence"
require_grep "\\[legal\\] Prepare rightsholder evidence package" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_legal"
require_grep "\\[qa\\] Define coverage threshold policy" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_qa_coverage_threshold"
require_grep "\\[security\\] Prepare external security/code review checklist" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_security_review"
require_grep "\\[compat\\] Test Russian OS compatibility matrix" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_compat_matrix"
require_grep "\\[ops\\] Validate release artifacts storage in RF" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_artifact_storage"
require_grep "\\[docs\\] Refresh public demo pack and screenshots" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_demo_pack"
require_grep "\\[pilot\\] Prepare Pilot Acceptance Checklist v2" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_pilot_acceptance_v2"
require_grep "\\[governance\\] Enable PR-based review workflow" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_pr_review_workflow"
require_grep "\\[governance\\] Add branch protection policy" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issue_branch_protection_policy"
require_grep "Acceptance criteria" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issues_acceptance_criteria"
require_grep "planned" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issues_status_planned"
require_grep "Do not mark restore test as completed until restore evidence exists" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issues_restore_guardrail"
require_grep "Do not mark .*awatch-build-01.* as ready until provisioning evidence exists" "docs/PUBLIC_ISSUES_PLAN_RU.md" "issues_build_runner_guardrail"
require_grep "public mirror validation only" "SECURITY.md" "security_public_mirror_validation"
require_grep "public mirror validation only" "CONTRIBUTING.md" "contributing_public_mirror_validation"
require_grep "public mirror validation only" "ROADMAP.md" "roadmap_public_mirror_validation"
require_grep "fail-closed" "docs/SECURITY_SCANNING_POLICY_RU.md" "security_scanning_policy_fail_closed"
require_grep "public-secret-scan: allow dummy" "docs/SECURITY_SCANNING_POLICY_RU.md" "security_scanning_policy_allow_comment"
require_grep "scripts/public_secret_pattern_check\\.py" ".github/workflows/security.yml" "security_workflow_local_secret_scanner"
require_grep "cargo audit|cargo deny|secret-pattern" ".github/workflows/security.yml" "security_workflow_checks"
require_grep "cargo llvm-cov" ".github/workflows/coverage.yml" "coverage_workflow_llvm_cov"
require_grep "cargo fmt --all --check" ".github/workflows/ci.yml" "ci_workflow_fmt"

scan_files=(
  "$ROOT/README.md"
  "$ROOT/docs/PROJECT_STATUS_RU.md"
  "$REGISTRY_DIR"/*.md
  "$REGISTRY_DIR"/*.json
  "$ROOT/docs/QUALITY_STATUS_RU.md"
  "$ROOT/docs/SECURITY_SCANNING_POLICY_RU.md"
  "$ROOT/docs/REVIEW_CHECKLIST_RU.md"
  "$ROOT/docs/RESIDUAL_RISKS_RU.md"
  "$ROOT/docs/PUBLIC_ISSUES_PLAN_RU.md"
  "$ROOT/docs/BRANCH_PROTECTION_POLICY_RU.md"
  "$ROOT/SECURITY.md"
  "$ROOT/CONTRIBUTING.md"
  "$ROOT/ROADMAP.md"
  "$ROOT/deny.toml"
  "$ROOT/.github/workflows"/*.yml
  "$ROOT/.github/ISSUE_TEMPLATE"/*.yml
  "$ROOT/.github/CODEOWNERS"
  "$ROOT/.github/pull_request_template.md"
  "$ROOT/scripts/build_release_evidence.sh"
  "$ROOT/scripts/check_release_evidence.sh"
)

claim_scan_files=(
  "$ROOT/README.md"
  "$ROOT/docs/PROJECT_STATUS_RU.md"
  "$REGISTRY_DIR"/*.md
  "$REGISTRY_DIR"/*.json
  "$ROOT/docs/QUALITY_STATUS_RU.md"
  "$ROOT/docs/SECURITY_SCANNING_POLICY_RU.md"
  "$ROOT/docs/REVIEW_CHECKLIST_RU.md"
  "$ROOT/docs/RESIDUAL_RISKS_RU.md"
  "$ROOT/docs/PUBLIC_ISSUES_PLAN_RU.md"
  "$ROOT/docs/BRANCH_PROTECTION_POLICY_RU.md"
  "$ROOT/SECURITY.md"
  "$ROOT/CONTRIBUTING.md"
  "$ROOT/ROADMAP.md"
)

if command -v python3 >/dev/null 2>&1; then
  if ! python3 "$ROOT/scripts/public_secret_pattern_check.py" >/tmp/registry_public_secret_scan.$$ 2>&1; then
    fail "public_secret_pattern_check:$(cat /tmp/registry_public_secret_scan.$$)"
  fi
  rm -f /tmp/registry_public_secret_scan.$$
else
  printf 'warning: python3 not found; skipped public_secret_pattern_check\n' >&2
fi

if grep -RInEi "(password|passwd|pwd|token|secret|api[_-]?key|private[[:space:]_-]?key)[[:space:]]*[:=][[:space:]]*['\"]?[A-Za-z0-9_./+=-]{8,}" "${scan_files[@]}" >/tmp/registry_secret_like.$$ 2>/dev/null; then
  fail "secret_like_value:$(cat /tmp/registry_secret_like.$$)"
fi
rm -f /tmp/registry_secret_like.$$

if grep -RInEi "(ФСТЭК|ФСБ).{0,80}(сертифицирован|сертификация|сертификат).{0,80}(есть|получен|имеется|подтвержден)|сертифицированное[[:space:]]+СЗИ" "${claim_scan_files[@]}" \
  | grep -Eiv "(does not claim|not_claimed|forbidden_claim|не заявляет|не фиксируется|не является|не утверждает|нельзя)" \
  >/tmp/registry_forbidden_fstec_fsb.$$ 2>/dev/null; then
  fail "forbidden_claim_fstec_fsb_certification:$(cat /tmp/registry_forbidden_fstec_fsb.$$)"
fi
rm -f /tmp/registry_forbidden_fstec_fsb.$$

if grep -RInEi "(заменяет|replacement for|replaces).{0,80}(SIEM|DLP)|((SIEM|DLP).{0,80}(replacement|заменяет))" "${claim_scan_files[@]}" \
  | grep -Eiv "(не |not |are not|does not|not_claimed|не является|не подменяет|не заявляет|forbidden|not_made)" \
  >/tmp/registry_forbidden_replacement.$$ 2>/dev/null; then
  fail "forbidden_claim_siem_dlp_replacement:$(cat /tmp/registry_forbidden_replacement.$$)"
fi
rm -f /tmp/registry_forbidden_replacement.$$

if grep -RInEi "(ML/LLM-based detection|LLM-based detection|ML-based detection|automatic remediation)" "${claim_scan_files[@]}" \
  | grep -Eiv "(forbidden|not_made|not_claimed|no claim|не заявляет|не фиксируется|не используется|does not claim)" \
  >/tmp/registry_forbidden_ai_auto.$$ 2>/dev/null; then
  fail "forbidden_claim_ai_or_automatic_remediation:$(cat /tmp/registry_forbidden_ai_auto.$$)"
fi
rm -f /tmp/registry_forbidden_ai_auto.$$

if grep -RInEi "(юридически заверш(е|ё)нн?ая регистрация|принят.*в реестр|реестр.*заверш(е|ё)н)" "${claim_scan_files[@]}" \
  | grep -Eiv "(не |нельзя|does not|not |не утверждает|не является|не подтверждает|forbidden)" \
  >/tmp/registry_forbidden_legal_done.$$ 2>/dev/null; then
  fail "forbidden_claim_legal_registry_completion:$(cat /tmp/registry_forbidden_legal_done.$$)"
fi
rm -f /tmp/registry_forbidden_legal_done.$$

if grep -RInEi "(branch protection).{0,120}(enabled|включ(е|ё)н|настроен|active)" "$ROOT/README.md" "$ROOT/docs/PROJECT_STATUS_RU.md" "$ROOT/docs/BRANCH_PROTECTION_POLICY_RU.md" "$REGISTRY_DIR"/*.md \
  | grep -Eiv "(not |не |не утверждает|not claimed|advisory|until repository settings|если применимо|recommended)" \
  >/tmp/registry_forbidden_branch_protection_enabled.$$ 2>/dev/null; then
  fail "forbidden_claim_branch_protection_enabled:$(cat /tmp/registry_forbidden_branch_protection_enabled.$$)"
fi
rm -f /tmp/registry_forbidden_branch_protection_enabled.$$

if grep -RInEi "(external|visible|peer).{0,80}(review).{0,120}(active|performed|completed|done|выполняется|проведен|провед(е|ё)н|активен)" "$ROOT/README.md" "$ROOT/docs/PROJECT_STATUS_RU.md" "$ROOT/docs/RESIDUAL_RISKS_RU.md" "$REGISTRY_DIR"/*.md \
  | grep -Eiv "(not |не |pending|still pending|не утверждает|not claimed|until public reviewed PRs)" \
  >/tmp/registry_forbidden_external_review_active.$$ 2>/dev/null; then
  fail "forbidden_claim_external_review_active:$(cat /tmp/registry_forbidden_external_review_active.$$)"
fi
rm -f /tmp/registry_forbidden_external_review_active.$$

if grep -RInEi "(restore(_| )?test|restore_tested|тестов(ое|ого)[[:space:]]+восстановлен).{0,120}(completed|done|passed|true|выполнен|готов|подтвержден)" "$ROOT/docs/RESIDUAL_RISKS_RU.md" "$ROOT/docs/PUBLIC_ISSUES_PLAN_RU.md" "$ROOT/docs/PROJECT_STATUS_RU.md" "$REGISTRY_DIR"/*.md "$REGISTRY_DIR"/*.json \
  | grep -Eiv "(not |не |false|pending|not completed|not claimed|не выполнен|еще не выполнен|ещё не выполнен|не заяв|не готов|until evidence|restore_tested.*false|если.*выполнен)" \
  >/tmp/registry_forbidden_restore_done.$$ 2>/dev/null; then
  fail "forbidden_claim_restore_test_completed:$(cat /tmp/registry_forbidden_restore_done.$$)"
fi
rm -f /tmp/registry_forbidden_restore_done.$$

if grep -RInEi "(build-runner|awatch-build-01).{0,120}(ready|production_ready|provisioned|готов|поднят|развернут|запущен)" "$ROOT/docs/RESIDUAL_RISKS_RU.md" "$ROOT/docs/PUBLIC_ISSUES_PLAN_RU.md" "$ROOT/docs/PROJECT_STATUS_RU.md" "$REGISTRY_DIR"/*.md "$REGISTRY_DIR"/*.json \
  | grep -Eiv "(not |не |planned|pending|not ready|not claimed|не готов|пока|until provisioning evidence|status != \"production_ready\"|requires|требует|подготовить|prepare)" \
  >/tmp/registry_forbidden_build_runner_ready.$$ 2>/dev/null; then
  fail "forbidden_claim_build_runner_ready:$(cat /tmp/registry_forbidden_build_runner_ready.$$)"
fi
rm -f /tmp/registry_forbidden_build_runner_ready.$$

if grep -RInEi "(release evidence (build|package)|first release evidence|перв(ый|ого).{0,40}release evidence).{0,120}(completed|done|produced|выполнен|готов|сформирован|создан)" "$ROOT/docs/RESIDUAL_RISKS_RU.md" "$ROOT/docs/PUBLIC_ISSUES_PLAN_RU.md" "$ROOT/docs/PROJECT_STATUS_RU.md" "$REGISTRY_DIR"/*.md "$REGISTRY_DIR"/*.json \
  | grep -Eiv "(not |не |pending|not claimed|not yet|еще не|ещё не|не выполнен|не заяв|requires|требует|produce first|first real|scripts exist|automation.*partially done)" \
  >/tmp/registry_forbidden_release_evidence_done.$$ 2>/dev/null; then
  fail "forbidden_claim_release_evidence_completed:$(cat /tmp/registry_forbidden_release_evidence_done.$$)"
fi
rm -f /tmp/registry_forbidden_release_evidence_done.$$

if grep -RInEi "(community adoption).{0,120}(high|strong|mature|высок|сильн|зрел)" "$ROOT/docs/RESIDUAL_RISKS_RU.md" "$ROOT/docs/PUBLIC_ISSUES_PLAN_RU.md" "$ROOT/docs/PROJECT_STATUS_RU.md" "$ROOT/README.md" \
  | grep -Eiv "(low|низк|не |not |fake|do not claim|не заяв)" \
  >/tmp/registry_forbidden_fake_community.$$ 2>/dev/null; then
  fail "forbidden_claim_fake_community_adoption:$(cat /tmp/registry_forbidden_fake_community.$$)"
fi
rm -f /tmp/registry_forbidden_fake_community.$$

if ((${#failures[@]} > 0)); then
  printf 'registry_readiness_check=fail\n'
  for failure in "${failures[@]}"; do
    printf '%s\n' "$failure"
  done
  exit 2
fi

printf 'registry_readiness_check=ok\n'
printf 'checked_files=%d\n' "${#required_files[@]}"
