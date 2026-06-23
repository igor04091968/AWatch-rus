#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/docs/public-issues/public-issues-manifest.json"

if [[ "${CONFIRM_CREATE_GITHUB_ISSUES:-}" != "YES" ]]; then
  printf 'create_public_issues=refused\n' >&2
  printf 'Set CONFIRM_CREATE_GITHUB_ISSUES=YES to create GitHub issues.\n' >&2
  printf 'Run scripts/prepare_public_issues.sh first and review the issue bodies.\n' >&2
  exit 2
fi

if ! command -v gh >/dev/null 2>&1; then
  printf 'create_public_issues=fail\n' >&2
  printf 'gh CLI is not installed or not in PATH.\n' >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  printf 'create_public_issues=fail\n' >&2
  printf 'jq is required for manifest-driven issue creation.\n' >&2
  exit 2
fi

if [[ ! -s "$MANIFEST" ]]; then
  printf 'create_public_issues=fail\n' >&2
  printf 'Missing manifest: docs/public-issues/public-issues-manifest.json\n' >&2
  exit 2
fi

jq -e . "$MANIFEST" >/dev/null
jq -e '
  (.status == "planned_issue_templates_ready" or .status == "public_issue_urls_recorded")
  and .github_issue_tracker == "manual_or_gh_cli_creation_required"
  and .github_role == "public_mirror_validation_only"
  and .registry_release_evidence == "requires_russian_build_runner"
  and (.issues | length == 12)
  and all(.issues[];
    (
      .status == "ready_to_create"
      and .github_issue_url == null
    )
    or (
      .status == "created"
      and (.github_issue_url | type == "string")
      and (.github_issue_url | test("^https://github\\.com/igor04091968/AWatch-rus/issues/[0-9]+$"))
    )
  )
' "$MANIFEST" >/dev/null

gh auth status >/dev/null

existing_labels="$(mktemp)"
trap 'rm -f "$existing_labels"' EXIT
gh label list --limit 500 --json name --jq '.[].name' >"$existing_labels"

while IFS= read -r label; do
  if ! grep -Fxq "$label" "$existing_labels"; then
    gh label create "$label" --color "ededed" --description "AWatch-rus public governance label"
    printf '%s\n' "$label" >>"$existing_labels"
  fi
done < <(jq -r '.issues[] | select(.status == "ready_to_create") | .labels[]' "$MANIFEST" | sort -u)

created=0

while IFS=$'\t' read -r title labels source; do
  body="$ROOT/$source"
  if [[ ! -s "$body" ]]; then
    printf 'create_public_issues=fail\n' >&2
    printf 'Missing issue body: %s\n' "$source" >&2
    exit 2
  fi
  url="$(gh issue create --title "$title" --label "$labels" --body-file "$body")"
  printf 'created_issue=%s\n' "$url"
  created=$((created + 1))
done < <(
  jq -r '.issues[] | select(.status == "ready_to_create") | [.title, (.labels | join(",")), .source] | @tsv' "$MANIFEST"
)

printf 'create_public_issues=ok\n'
printf 'created=%d\n' "$created"
printf 'Update docs/public-issues/public-issues-manifest.json with the printed issue URLs.\n'
