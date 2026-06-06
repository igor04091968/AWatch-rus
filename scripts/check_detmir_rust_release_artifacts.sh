#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT_DIR/adk-rust/target}"
RELEASE_DIR="$TARGET_ROOT/release"

required_bins=(
  detmir-status
  detmir-adk-status
  detmir-check
  detmir-dlp
  detmir-auto
  detmir-heal-safe
  tsj-guardian-watchdog
  tsj-guardian-status
  aw-rus-healthd
  aw-db-health
  aw-db-maintenance
  aw-ensure-reliability
  aw-health-check
  aw-linux-install
  aw-prune-local-state
  check-aw-data
  check-aw-full
  check-install-kit-vs-repo
  validate-install-kit
  verify-innosetup-installer
  rebuild-install-kit
  quality-gate
  extract-ioc-from-sigma
  merge-aw-server-dbs
  prod-backup-restore
  prod-rollout
  rdp-worktime-report
  aw-contour-smoke
  aw-browser-smoke
  diag-and-manual-restart
  detmir-grafana-check
  detmir-readiness
  detmir-portal
  dlp-health-check
  dlp-content-analyzer
  dlp-admin-cli
  dlp-policy-engine
  dlp-case-management
  dlp-compliance
  dlp-aggregator
  dlp-syslog-forwarder
  dlp-webhook-sender
  dlp-cef-exporter
  dlp-influx-exporter
  worktime-influx-exporter
  worktime-prewarm
  worktime-ui-bridge
  worktime-autoheal
  worktime-api
  aw-slo-monitor
  aw-hayabusa-case-alert-rust
  aw-hayabusa-link-case-rust
  aw-hayabusa-from-windows-rust
  aw-hayabusa-autoprocess-rust
  aw-1c-ingest
)

missing=0
for bin in "${required_bins[@]}"; do
  if [[ -x "$RELEASE_DIR/$bin" ]]; then
    printf 'OK %s\n' "$bin"
  else
    printf 'MISSING %s (%s)\n' "$bin" "$RELEASE_DIR/$bin" >&2
    missing=1
  fi
done

if (( missing != 0 )); then
  cat >&2 <<EOF

Missing DetMir Rust release artifacts.
Build them with:
  cd "$ROOT_DIR/adk-rust"
  CARGO_TARGET_DIR="$TARGET_ROOT" cargo build --release --workspace
EOF
  exit 1
fi

echo "detmir rust release artifacts: OK ($RELEASE_DIR)"
