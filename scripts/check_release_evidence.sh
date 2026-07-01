#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  printf 'usage: %s <release-evidence-dir>\n' "$0" >&2
  exit 2
fi

EVIDENCE_DIR="$1"
failures=()

fail() {
  failures+=("$1")
}

require_file() {
  local path="$1"
  if [[ ! -s "$EVIDENCE_DIR/$path" ]]; then
    fail "missing_or_empty:$path"
  fi
}

require_dir() {
  local path="$1"
  if [[ ! -d "$EVIDENCE_DIR/$path" ]]; then
    fail "missing_directory:$path"
  fi
}

if [[ ! -d "$EVIDENCE_DIR" ]]; then
  printf 'release_evidence_check=fail\nmissing_directory:%s\n' "$EVIDENCE_DIR"
  exit 2
fi

require_file "release-evidence-manifest.json"
require_file "RELEASE_EVIDENCE_REPORT_RU.md"
require_file "SHA256SUMS"
require_dir "logs"
require_dir "artifacts"

if [[ -s "$EVIDENCE_DIR/SHA256SUMS" ]]; then
  (cd "$EVIDENCE_DIR" && sha256sum -c SHA256SUMS >/dev/null) || fail "sha256sum_check_failed"
fi

if [[ -s "$EVIDENCE_DIR/release-evidence-manifest.json" ]]; then
  if command -v jq >/dev/null 2>&1; then
    jq -e . "$EVIDENCE_DIR/release-evidence-manifest.json" >/dev/null || fail "invalid_json:release-evidence-manifest.json"
    jq -e '
      .product == "AWatch-rus"
      and (.release_version | type == "string" and length > 0)
      and (.release_commit | type == "string" and test("^[0-9a-f]{40}$"))
      and (.release_commit_input | type == "string" and length > 0)
      and (.source_date_epoch | type == "number")
      and (.build_time_utc | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
      and (.build_runner | type == "string" and length > 0)
      and .primary_source_repository == "https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus"
      and .github_role == "public_mirror_only"
      and (.generated_at | type == "string" and length > 0)
      and (.checks | type == "array")
      and (.artifacts | type == "array")
    ' "$EVIDENCE_DIR/release-evidence-manifest.json" >/dev/null || fail "manifest_required_fields"
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$EVIDENCE_DIR/release-evidence-manifest.json" <<'PY' || fail "manifest_required_fields"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

required = {
    "product": "AWatch-rus",
    "primary_source_repository": "https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus",
    "github_role": "public_mirror_only",
}
for key, value in required.items():
    if data.get(key) != value:
        raise SystemExit(f"{key} mismatch")
for key in ("release_version", "release_commit", "release_commit_input", "build_time_utc", "build_runner", "generated_at"):
    if not isinstance(data.get(key), str) or not data[key]:
        raise SystemExit(f"{key} missing")
if len(data["release_commit"]) != 40 or any(char not in "0123456789abcdef" for char in data["release_commit"]):
    raise SystemExit("release_commit must be a full lowercase git SHA")
if not isinstance(data.get("source_date_epoch"), int):
    raise SystemExit("source_date_epoch missing")
if not isinstance(data.get("checks"), list):
    raise SystemExit("checks missing")
if not isinstance(data.get("artifacts"), list):
    raise SystemExit("artifacts missing")
PY
  else
    fail "json_validator_missing:jq_or_python3_required"
  fi
fi

if [[ ! -s "$EVIDENCE_DIR/cargo-metadata.json" ]] && ! grep -Eiq 'cargo_metadata.*skipped' "$EVIDENCE_DIR/skipped-checks.txt" 2>/dev/null; then
  fail "missing_cargo_metadata_or_documented_skip"
fi

if [[ ! -s "$EVIDENCE_DIR/cargo-tree.txt" ]] && ! grep -Eiq 'cargo_tree.*skipped' "$EVIDENCE_DIR/skipped-checks.txt" 2>/dev/null; then
  fail "missing_cargo_tree_or_documented_skip"
fi

if ! find "$EVIDENCE_DIR/artifacts" -maxdepth 1 -type f -name '*source*.tar.gz' | grep -q .; then
  fail "missing_source_archive"
fi

if ! find "$EVIDENCE_DIR/artifacts" -maxdepth 1 -type f \( -name '*binaries*.tar.gz' -o -name '*binaries*.tar.gz.skip' \) | grep -q .; then
  fail "missing_binary_archive_or_documented_skip"
fi

scan_files=(
  "$EVIDENCE_DIR/RELEASE_EVIDENCE_REPORT_RU.md"
  "$EVIDENCE_DIR/release-evidence-manifest.json"
)

if grep -RInEi "(ФСТЭК|ФСБ).{0,80}(сертифицирован|сертификация|сертификат).{0,80}(есть|получен|имеется|подтвержден)|сертифицированное[[:space:]]+СЗИ" "${scan_files[@]}" >/tmp/release_evidence_forbidden_cert.$$ 2>/dev/null; then
  fail "forbidden_claim_certification:$(cat /tmp/release_evidence_forbidden_cert.$$)"
fi
rm -f /tmp/release_evidence_forbidden_cert.$$

if grep -RInEi "(replaces|заменяет).{0,80}(SIEM|DLP)|(SIEM|DLP).{0,80}(replacement|заменяет)" "${scan_files[@]}" \
  | grep -Eiv "(does not|not |not_claimed|не |forbidden|не заявляет)" \
  >/tmp/release_evidence_forbidden_replace.$$ 2>/dev/null; then
  fail "forbidden_claim_siem_dlp_replacement:$(cat /tmp/release_evidence_forbidden_replace.$$)"
fi
rm -f /tmp/release_evidence_forbidden_replace.$$

if ((${#failures[@]} > 0)); then
  printf 'release_evidence_check=fail\n'
  for failure in "${failures[@]}"; do
    printf '%s\n' "$failure"
  done
  exit 2
fi

printf 'release_evidence_check=ok\n'
