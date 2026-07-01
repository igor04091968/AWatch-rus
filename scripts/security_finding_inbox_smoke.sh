#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${SECURITY_FINDING_INBOX_BIN:-}"
SAMPLE="${1:-$ROOT_DIR/configs/security/security-finding.example.json}"
TMP_DIR="$(mktemp -d /tmp/security-finding-inbox-smoke.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

if [[ -z "$BIN" ]]; then
  for candidate in \
    "${CARGO_TARGET_DIR:-}/debug/security-finding-inbox" \
    "${CARGO_TARGET_DIR:-}/release/security-finding-inbox" \
    "$ROOT_DIR/adk-rust/target/debug/security-finding-inbox" \
    "$ROOT_DIR/adk-rust/target/release/security-finding-inbox" \
    "/usr/local/bin/security-finding-inbox"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
      BIN="$candidate"
      break
    fi
  done
fi

if [[ -z "$BIN" ]]; then
  printf 'security-finding-inbox binary not found. Build: cargo build --manifest-path adk-rust/Cargo.toml -p security-finding-inbox\n' >&2
  exit 2
fi

"$BIN" schema | grep -q 'CREATE TABLE IF NOT EXISTS analytics_1c.security_findings'
"$BIN" validate --input "$SAMPLE" | python3 -c '
import json
import sys
payload = json.load(sys.stdin)
assert payload["ok"] is True
assert payload["rows"] == 1
assert payload["finding_ids"][0].startswith("sf-")
print("security_finding_validate=ok")
'

"$BIN" ingest --input "$SAMPLE" --dry-run | python3 -c '
import json
import sys
payload = json.load(sys.stdin)
assert payload["ok"] is True
assert payload["dry_run"] is True
assert payload["rows"] == 1
print("security_finding_ingest_dry_run=ok")
'

mkdir -p "$TMP_DIR/hayabusa-report"
cat >"$TMP_DIR/hayabusa-report/timeline.jsonl" <<'EOF'
{"Level":"high","RuleTitle":"PowerShell Credential Dump","Timestamp":"2026-06-25T10:00:00Z"}
{"Level":"crit","RuleTitle":"Suspicious Credential Access","Timestamp":"2026-06-25T10:01:00Z"}
EOF
cat >"$TMP_DIR/hayabusa-report/logon-summary-failed.csv" <<'EOF'
header
1
2
EOF
cat >"$TMP_DIR/latest-intake.json" <<EOF
{
  "host": "HOST-EXAMPLE",
  "status": "ok",
  "intake_id": "smoke-intake-001",
  "package_path": "$TMP_DIR/HOST-EXAMPLE.zip",
  "sha256": "demo",
  "report_dir": "$TMP_DIR/hayabusa-report"
}
EOF
"$BIN" ingest-hayabusa --intake "$TMP_DIR/latest-intake.json" --min-severity low --dry-run | python3 -c '
import json
import sys
payload = json.load(sys.stdin)
assert payload["ok"] is True
assert payload["dry_run"] is True
assert payload["rows"] == 1
print("security_finding_hayabusa_ingest_dry_run=ok")
'

cat >"$TMP_DIR/velociraptor.jsonl" <<'EOF'
{"Hostname":"HOST-EXAMPLE","Artifact":"Windows.Hayabusa.Monitoring","Severity":"high","Message":"Velociraptor smoke finding","User":"user-example"}
EOF
"$BIN" ingest-velociraptor-json --input "$TMP_DIR/velociraptor.jsonl" --dry-run | python3 -c '
import json
import sys
payload = json.load(sys.stdin)
assert payload["ok"] is True
assert payload["dry_run"] is True
assert payload["rows"] == 1
print("security_finding_velociraptor_ingest_dry_run=ok")
'

"$BIN" workflow \
  --finding-id sf-demo \
  --event-type approved \
  --actor smoke \
  --comment "dry-run approval" \
  --dry-run | python3 -c '
import json
import sys
payload = json.load(sys.stdin)
assert payload["ok"] is True
assert payload["dry_run"] is True
assert payload["event_type"] == "approved"
print("security_finding_workflow_dry_run=ok")
'

"$BIN" executor --help >/dev/null
printenv SECURITY_FINDING_INBOX_SKIP_EXECUTOR_SMOKE >/dev/null 2>&1 || \
  printf 'security_finding_executor_cli=ok\n'
