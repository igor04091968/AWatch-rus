#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RC_NAME="${1:-}"
if [[ -z "$RC_NAME" ]]; then
  cat >&2 <<'EOF'
usage: bash scripts/build_release_candidate.sh <rc-name>
example: bash scripts/build_release_candidate.sh v1.0.2-rc1
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
