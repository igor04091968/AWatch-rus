#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE="${CONTAINMENT_ENGINE_BIN:-}"
POLICY="${1:-$ROOT_DIR/configs/containment-policy.example.json}"
FINDING="${2:-$ROOT_DIR/configs/containment-finding.example.json}"
FIREWALL_REQUEST="${3:-$ROOT_DIR/configs/windows-firewall-containment-request.example.json}"
TMP_DIR="$(mktemp -d /tmp/containment-shadow-smoke.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

if [[ -z "$ENGINE" ]]; then
  for candidate in \
    "${CARGO_TARGET_DIR:-}/debug/containment-engine" \
    "${CARGO_TARGET_DIR:-}/release/containment-engine" \
    "$ROOT_DIR/adk-rust/target/debug/containment-engine" \
    "$ROOT_DIR/adk-rust/target/release/containment-engine" \
    "/usr/local/bin/containment-engine"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
      ENGINE="$candidate"
      break
    fi
  done
fi

if [[ -z "$ENGINE" ]]; then
  printf 'containment-engine binary not found. Build: cargo build --manifest-path adk-rust/Cargo.toml -p containment-engine\n' >&2
  exit 2
fi

validate_payload() {
  local expected_status="$1"
  python3 -c '
import json
import sys

expected_status = sys.argv[1]
payload = json.load(sys.stdin)
if payload.get("would_mutate") is not False:
    raise SystemExit("containment smoke failed: would_mutate must be false")
status = payload.get("decision_status")
if status != expected_status:
    raise SystemExit(f"containment smoke failed: expected {expected_status!r}, got {status!r}")
print(f"containment_shadow_smoke=ok status={status}")
' "$expected_status"
}

disabled_out="$("$ENGINE" decide --policy "$POLICY" --finding "$FINDING" --pretty)"
printf '%s\n' "$disabled_out"
validate_payload "disabled" <<<"$disabled_out"

shadow_policy="$TMP_DIR/containment-policy-shadow.json"
python3 - "$POLICY" "$shadow_policy" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
payload["enabled"] = True
payload["mode"] = "shadow"
json.dump(payload, open(sys.argv[2], "w", encoding="utf-8"), ensure_ascii=False, indent=2)
PY

shadow_out="$("$ENGINE" decide --policy "$shadow_policy" --finding "$FINDING" --pretty)"
printf '%s\n' "$shadow_out"
validate_payload "shadow_recommended" <<<"$shadow_out"

firewall_plan="$TMP_DIR/windows-firewall-plan.json"
"$ENGINE" windows-firewall plan --request "$FIREWALL_REQUEST" --pretty >"$firewall_plan"
cat "$firewall_plan"

python3 - "$firewall_plan" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
if payload.get("executor") != "windows_firewall":
    raise SystemExit("firewall smoke failed: executor must be windows_firewall")
if payload.get("blockers"):
    raise SystemExit(f"firewall smoke failed: unexpected blockers {payload['blockers']!r}")
if not payload.get("apply_commands") or not payload.get("rollback_commands"):
    raise SystemExit("firewall smoke failed: apply/rollback commands must exist")
print("windows_firewall_plan_smoke=ok")
PY

firewall_apply_out="$("$ENGINE" windows-firewall apply --plan "$firewall_plan" --confirm-apply YES --pretty)"
printf '%s\n' "$firewall_apply_out"

python3 -c '
import json
import sys

payload = json.load(sys.stdin)
if payload.get("execution_status") != "dry_run_commands_ready":
    raise SystemExit("firewall apply smoke failed: expected dry_run_commands_ready")
if payload.get("would_mutate") is not False:
    raise SystemExit("firewall apply smoke failed: dry-run must not mutate")
print("windows_firewall_apply_dry_run_smoke=ok")
' <<<"$firewall_apply_out"
