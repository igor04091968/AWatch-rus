#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISSUES_DIR="$ROOT/docs/public-issues"
MANIFEST="$ISSUES_DIR/public-issues-manifest.json"

expected_files=(
  "001-registry-gitea-restore-test.md"
  "002-registry-russian-build-runner.md"
  "003-release-evidence-package.md"
  "004-legal-rightsholder-package.md"
  "005-coverage-threshold-policy.md"
  "006-external-security-code-review-checklist.md"
  "007-russian-os-compatibility-matrix.md"
  "008-release-artifacts-storage-rf.md"
  "009-public-demo-pack-refresh.md"
  "010-pilot-acceptance-checklist-v2.md"
  "011-governance-pr-based-review-workflow.md"
  "012-governance-branch-protection-policy.md"
)

required_sections=(
  "Title"
  "Labels"
  "Purpose"
  "Scope"
  "Non-goals"
  "Acceptance criteria"
  "Evidence required"
  "Safety/privacy guardrails"
  "Registry-positioning guardrails"
)

failures=()

fail() {
  failures+=("$1")
}

section_value() {
  local section="$1"
  local file="$2"
  awk -v section="$section" '
    $0 == "## " section { found = 1; next }
    found && /^## / { exit }
    found && NF { print; exit }
  ' "$file"
}

labels_for_gh() {
  printf '%s' "$1" \
    | tr -d '`' \
    | tr ',' '\n' \
    | sed -E 's/^[[:space:]]+|[[:space:]]+$//g' \
    | awk 'NF { printf "%s%s", sep, $0; sep="," }'
}

if [[ ! -d "$ISSUES_DIR" ]]; then
  fail "missing_directory:docs/public-issues"
fi

if [[ ! -s "$MANIFEST" ]]; then
  fail "missing_or_empty:docs/public-issues/public-issues-manifest.json"
fi

for name in "${expected_files[@]}"; do
  file="$ISSUES_DIR/$name"
  if [[ ! -s "$file" ]]; then
    fail "missing_or_empty:docs/public-issues/$name"
    continue
  fi
  for section in "${required_sections[@]}"; do
    if ! grep -Eq "^## ${section}$" "$file"; then
      fail "missing_section:docs/public-issues/$name:$section"
    fi
  done
done

if [[ -s "$MANIFEST" ]] && command -v jq >/dev/null 2>&1; then
  jq -e . "$MANIFEST" >/dev/null || fail "invalid_json:docs/public-issues/public-issues-manifest.json"
  jq -e '
    .status == "planned_issue_templates_ready"
    and .github_issue_tracker == "manual_or_gh_cli_creation_required"
    and .github_role == "public_mirror_validation_only"
    and .registry_release_evidence == "requires_russian_build_runner"
    and (.issues | length == 12)
    and all(.issues[]; .status == "ready_to_create" and .github_issue_url == null)
  ' "$MANIFEST" >/dev/null || fail "manifest_required_fields"
  while IFS= read -r source; do
    [[ -s "$ROOT/$source" ]] || fail "manifest_source_missing:$source"
  done < <(jq -r '.issues[].source' "$MANIFEST")
elif [[ -s "$MANIFEST" ]]; then
  printf 'warning: jq not found; JSON syntax validation skipped\n' >&2
fi

if ((${#failures[@]} > 0)); then
  printf 'public_issues_prepare=fail\n'
  for failure in "${failures[@]}"; do
    printf '%s\n' "$failure"
  done
  exit 2
fi

printf 'public_issues_prepare=ok\n'
printf 'status=ready_to_create\n'
printf 'issue_templates=%d\n' "${#expected_files[@]}"
printf '\n'
printf 'Manual gh CLI commands, after maintainer review and gh auth:\n'

for name in "${expected_files[@]}"; do
  file="$ISSUES_DIR/$name"
  title="$(section_value "Title" "$file")"
  labels="$(labels_for_gh "$(section_value "Labels" "$file")")"
  rel="docs/public-issues/$name"
  printf 'gh issue create --title %q --label %q --body-file %q\n' "$title" "$labels" "$rel"
done

printf '\n'
printf 'After creating issues, update docs/public-issues/public-issues-manifest.json with github_issue_url values.\n'
