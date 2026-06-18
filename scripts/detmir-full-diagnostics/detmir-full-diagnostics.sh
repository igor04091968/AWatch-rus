#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ENV_FILE="${DETMIR_FULL_DIAGNOSTICS_ENV_FILE:-$SCRIPT_DIR/detmir-full-diagnostics.env}"
if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

usage() {
  cat <<'USAGE'
Usage:
  scripts/detmir-full-diagnostics/detmir-full-diagnostics.sh [options]

Options:
  --scope daily|weekly|monthly   Scope of operational checks (default: daily)
  --output-dir PATH              Root output directory for all checks
  --quick                        Pass --quick to aw-contour-diag
  --skip-windows                 Skip WinRM checks in aw-contour-diag
  --no-support                   Skip detmir-support-run checks
  --no-aw-diagnostic             Skip aw-contour-diag
  --no-placeholder-check          Skip production placeholder guard check
  --help                         Show this help

Examples:
  scripts/detmir-full-diagnostics/detmir-full-diagnostics.sh --scope daily
  scripts/detmir-full-diagnostics/detmir-full-diagnostics.sh --scope weekly --quick
  scripts/detmir-full-diagnostics/detmir-full-diagnostics-daily.sh
USAGE
}

SCOPE="${DETMIR_FULL_DIAGNOSTICS_SCOPE:-daily}"
OUTPUT_DIR="${DETMIR_FULL_DIAGNOSTICS_OUTPUT_DIR:-$REPO_ROOT/output/detmir-full-diagnostics}"
RUN_AW_DIAG=1
RUN_SUPPORT=1
RUN_PLACEHOLDER_GUARD=1
AW_QUICK="${DETMIR_FULL_DIAGNOSTICS_AW_DIAG_QUICK:-0}"
AW_SKIP_WINDOWS="${DETMIR_FULL_DIAGNOSTICS_AW_DIAG_SKIP_WINDOWS:-0}"

AW_DIAG_SCRIPT="${DETMIR_FULL_DIAGNOSTICS_AW_DIAG_SCRIPT:-$SCRIPT_DIR/aw-contour-diag.sh}"
SUPPORT_RUN_SCRIPT="${DETMIR_FULL_DIAGNOSTICS_SUPPORT_RUN:-$SCRIPT_DIR/../detmir-support-run.sh}"
SUPPORT_ENV_FILE="${DETMIR_FULL_DIAGNOSTICS_SUPPORT_ENV:-$SCRIPT_DIR/../detmir-support.env}"
PLACEHOLDER_SCRIPT="${DETMIR_FULL_DIAGNOSTICS_PLACEHOLDER_SCRIPT:-$SCRIPT_DIR/../check_production_inventory_placeholders.sh}"
PLACEHOLDER_GUARD="${DETMIR_FULL_DIAGNOSTICS_PLACEHOLDER_GUARD:-1}"
PLACEHOLDER_PATHS="${DETMIR_FULL_DIAGNOSTICS_PLACEHOLDER_PATHS:-$REPO_ROOT/ansible:$REPO_ROOT/private-config}"

while (( $# > 0 )); do
  case "$1" in
    --scope)
      if (( $# < 2 )); then
        usage
        echo "error: --scope requires daily|weekly|monthly" >&2
        exit 2
      fi
      SCOPE="$2"
      shift 2
      ;;
    --output-dir)
      if (( $# < 2 )); then
        usage
        echo "error: --output-dir requires path" >&2
        exit 2
      fi
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --quick)
      AW_QUICK=1
      shift
      ;;
    --skip-windows)
      AW_SKIP_WINDOWS=1
      shift
      ;;
    --no-support)
      RUN_SUPPORT=0
      shift
      ;;
    --no-aw-diagnostic)
      RUN_AW_DIAG=0
      shift
      ;;
    --no-placeholder-check)
      RUN_PLACEHOLDER_GUARD=0
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      usage
      echo "error: unknown arg '$1'" >&2
      exit 2
      ;;
  esac
done

if [[ "$SCOPE" != "daily" && "$SCOPE" != "weekly" && "$SCOPE" != "monthly" ]]; then
  echo "error: invalid scope '$SCOPE', expected daily|weekly|monthly" >&2
  exit 2
fi

if [[ "$RUN_PLACEHOLDER_GUARD" == "1" ]]; then
  RUN_PLACEHOLDER_GUARD="$PLACEHOLDER_GUARD"
fi

RUN_DIR="$OUTPUT_DIR/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$RUN_DIR"
LOG_FILE="$RUN_DIR/full-diagnostics.log"

exec > >(tee -a "$LOG_FILE") 2>&1

TOTAL=0
OK_COUNT=0
FAIL_COUNT=0

log() {
  printf '%s %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*"
}

run_step() {
  local step="$1"
  shift

  TOTAL=$((TOTAL + 1))
  log "----"
  log "STEP [$TOTAL]: $step"

  if "$@"; then
    OK_COUNT=$((OK_COUNT + 1))
    log "RESULT: OK"
    return 0
  fi

  FAIL_COUNT=$((FAIL_COUNT + 1))
  log "RESULT: FAIL"
  return 1
}

log "DetMir full diagnostics started"
log "Repository: $REPO_ROOT"
log "Scope: $SCOPE"
log "Output: $RUN_DIR"
log "Log: $LOG_FILE"

if [[ ! -x "$AW_DIAG_SCRIPT" ]] && (( RUN_AW_DIAG == 1 )); then
  log "WARN: aw-contour-diag script not found or not executable: $AW_DIAG_SCRIPT"
  log "      Диагностика AW-контуром будет пропущена."
  RUN_AW_DIAG=0
fi

if [[ ! -x "$SUPPORT_RUN_SCRIPT" ]] && (( RUN_SUPPORT == 1 )); then
  log "ERROR: support script not found or not executable: $SUPPORT_RUN_SCRIPT"
  exit 2
fi
if [[ ! -f "$SUPPORT_ENV_FILE" ]] && (( RUN_SUPPORT == 1 )); then
  log "WARN: support env file not found, defaults will be used from detmir-support-run defaults: $SUPPORT_ENV_FILE"
fi

if [[ ! -f "$PLACEHOLDER_SCRIPT" ]] && (( RUN_PLACEHOLDER_GUARD == 1 )); then
  log "WARN: placeholder guard script not found: $PLACEHOLDER_SCRIPT"
  RUN_PLACEHOLDER_GUARD=0
fi

if (( RUN_AW_DIAG == 1 )); then
  if [[ -x "$AW_DIAG_SCRIPT" ]]; then
    AW_ARGS=()
    if [[ "$AW_QUICK" == "1" ]]; then
      AW_ARGS+=(--quick)
    fi
    if [[ "$AW_SKIP_WINDOWS" == "1" ]]; then
      AW_ARGS+=(--skip-windows)
    fi
    run_step "aw-contour-diag (scope=$SCOPE)" "$AW_DIAG_SCRIPT" "${AW_ARGS[@]}" || true
  fi
fi

if (( RUN_SUPPORT == 1 )); then
  run_step "detmir-support-run --scope $SCOPE" \
    env \
      DETMIR_SUPPORT_ENV_FILE="$SUPPORT_ENV_FILE" \
      DETMIR_SUPPORT_OUTPUT_DIR="$RUN_DIR/support" \
    "$SUPPORT_RUN_SCRIPT" --scope "$SCOPE" --output-dir "$RUN_DIR/support" || true
fi

if [[ "$RUN_PLACEHOLDER_GUARD" == "1" ]]; then
  if [[ -z "${PLACEHOLDER_PATHS// }" ]]; then
    log "WARN: DETMIR_FULL_DIAGNOSTICS_PLACEHOLDER_PATHS is empty. Placeholder guard skipped."
  else
    run_step "check production placeholders" \
      env DETMIR_PRODUCTION_CONFIG_PATHS="$PLACEHOLDER_PATHS" \
      "$PLACEHOLDER_SCRIPT" --allow-missing || true
  fi
fi

SUMMARY_FILE="$RUN_DIR/summary.txt"
{
  printf "Scope: %s\n" "$SCOPE"
  printf "Total steps: %s\n" "$TOTAL"
  printf "OK: %s\n" "$OK_COUNT"
  printf "FAIL: %s\n" "$FAIL_COUNT"
  if (( FAIL_COUNT == 0 )); then
    printf "Result: OK\n"
  else
    printf "Result: FAIL\n"
  fi
} > "$SUMMARY_FILE"

log "Summary: $SUMMARY_FILE"
log "DetMir full diagnostics completed"

if (( FAIL_COUNT > 0 )); then
  exit 1
fi

exit 0
