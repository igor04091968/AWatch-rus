#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

print_cargo_target_dir() {
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
  else
    echo "CARGO_TARGET_DIR is not set; cargo default target dir will be used"
  fi
}

preflight_ok() {
  echo "[OK] $1"
}

preflight_fail() {
  echo "[FAIL] $1" >&2
  PREFLIGHT_FAILED=1
}

check_command() {
  local command_name="$1"
  if command -v "$command_name" >/dev/null 2>&1; then
    preflight_ok "command available: $command_name"
  else
    preflight_fail "missing command: $command_name"
  fi
}

check_file() {
  local path="$1"
  if [[ -f "$path" ]]; then
    preflight_ok "required file exists: $path"
  else
    preflight_fail "required file is missing: $path"
  fi
}

run_preflight() {
  PREFLIGHT_FAILED=0

  echo "release candidate preflight"
  print_cargo_target_dir

  check_command git
  check_command cargo
  check_command bash
  check_command node
  check_command sha256sum

  check_file scripts/generate_release_sbom_v0_2.sh
  check_file scripts/check_private_config_guard.sh
  check_file scripts/check_portal_contract_sync.mjs

  if command -v git >/dev/null 2>&1; then
    if git check-ignore -q dist/release-candidate/.preflight-probe; then
      preflight_ok "dist/ is ignored by git"
    else
      preflight_fail "dist/ is not ignored by git"
    fi
  else
    preflight_fail "cannot verify git ignore rules without git"
  fi

  case "$ROOT_DIR" in
    /mnt/*|/media/*)
      cat <<'EOF'
[HINT] Project is under /mnt or /media. If cargo fails on the mount with Operation not permitted, run the full RC build with a writable target dir:
       CARGO_TARGET_DIR=/home/igor/.cache/aw-rus-hardening-target bash scripts/build_release_candidate.sh v1.0.2-rc1
EOF
      ;;
  esac

  if [[ "$PREFLIGHT_FAILED" -ne 0 ]]; then
    echo "release candidate preflight: FAIL" >&2
    return 1
  fi

  echo "release candidate preflight: OK"
}

if [[ "${1:-}" == "--preflight" ]]; then
  run_preflight
  exit $?
fi

print_cargo_target_dir

RC_NAME="${1:-}"
if [[ -z "$RC_NAME" ]]; then
  cat >&2 <<'EOF'
usage: bash scripts/build_release_candidate.sh <rc-name>
example: bash scripts/build_release_candidate.sh v1.0.2-rc1

preflight: bash scripts/build_release_candidate.sh --preflight
EOF
  exit 2
fi

if [[ ! "$RC_NAME" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
  echo "invalid release candidate name: start with a letter or number; use only letters, numbers, dot, underscore, and hyphen" >&2
  exit 2
fi

if [[ -n "$(git status --porcelain --untracked-files=normal)" ]]; then
  echo "git working tree is not clean; commit, stash, or remove changes before building release candidate" >&2
  git status --short >&2
  exit 1
fi

OUT_DIR="$ROOT_DIR/dist/release-candidate/$RC_NAME"
if [[ -e "$OUT_DIR" ]]; then
  echo "release candidate output already exists: $OUT_DIR" >&2
  exit 1
fi

BUILD_SUCCESS=0
cleanup_on_failure() {
  status=$?
  if [[ $status -ne 0 && $BUILD_SUCCESS -ne 1 && -d "$OUT_DIR" ]]; then
    rm -rf "$OUT_DIR"
  fi
  exit "$status"
}
trap cleanup_on_failure EXIT

mkdir -p "$OUT_DIR"

git rev-parse HEAD > "$OUT_DIR/git-commit.txt"

cargo fmt --manifest-path adk-rust/Cargo.toml --all -- --check
cargo test --manifest-path adk-rust/Cargo.toml --workspace
cargo clippy --manifest-path adk-rust/Cargo.toml --workspace --all-targets -- -D warnings
cargo build --manifest-path adk-rust/Cargo.toml --workspace --release
bash scripts/quality-gate.sh
bash scripts/check_private_config_guard.sh
node scripts/check_portal_contract_sync.mjs

bash scripts/generate_release_sbom_v0_2.sh "$OUT_DIR"

(
  cd "$OUT_DIR"
  {
    printf '%s\n' "FILES.txt"
    find . -type f ! -name 'FILES.txt' ! -name 'SHA256SUMS.txt' -print \
      | sort \
      | sed 's#^\./##'
  } > FILES.txt

  find . -type f ! -name 'SHA256SUMS.txt' -print0 \
    | sort -z \
    | xargs -0 sha256sum > SHA256SUMS.txt
)

BUILD_SUCCESS=1
echo "release candidate built: $OUT_DIR"
