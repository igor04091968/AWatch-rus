#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export GIT_LFS_SKIP_SMUDGE=1

RELEASE_VERSION="${RELEASE_VERSION:-registry-candidate-$(date +%Y%m%d-%H%M%S)}"
RELEASE_COMMIT="${RELEASE_COMMIT:-$(git rev-parse HEAD)}"
OUTPUT_DIR="${OUTPUT_DIR:-release-evidence/${RELEASE_VERSION}}"
DOCS_ONLY="${DOCS_ONLY:-0}"
CARGO_MANIFEST_PATH="${CARGO_MANIFEST_PATH:-}"

if [[ -z "$CARGO_MANIFEST_PATH" ]]; then
  if [[ -f Cargo.toml ]]; then
    CARGO_MANIFEST_PATH="Cargo.toml"
  elif [[ -f adk-rust/Cargo.toml ]]; then
    CARGO_MANIFEST_PATH="adk-rust/Cargo.toml"
  fi
fi

if [[ -n "$CARGO_MANIFEST_PATH" ]]; then
  CARGO_WORKSPACE_DIR="$(cd "$(dirname "$CARGO_MANIFEST_PATH")" && pwd)"
else
  CARGO_WORKSPACE_DIR=""
fi

LOG_DIR="$OUTPUT_DIR/logs"
ARTIFACT_DIR="$OUTPUT_DIR/artifacts"
SKIP_FILE="$OUTPUT_DIR/skipped-checks.txt"
CHECKS_FILE="$OUTPUT_DIR/checks.tsv"

mkdir -p "$LOG_DIR" "$ARTIFACT_DIR"
: > "$SKIP_FILE"
: > "$CHECKS_FILE"

record_check() {
  local name="$1"
  local status="$2"
  local detail="${3:-}"
  printf '%s\t%s\t%s\n' "$name" "$status" "$detail" >> "$CHECKS_FILE"
}

run_logged() {
  local name="$1"
  shift
  local log="$LOG_DIR/${name}.log"
  {
    printf 'command:'
    printf ' %q' "$@"
    printf '\nstarted_at: %s\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$log"
  if "$@" >> "$log" 2>&1; then
    printf '\nfinished_at: %s\nstatus: ok\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$log"
    record_check "$name" "ok" "$log"
  else
    local status=$?
    printf '\nfinished_at: %s\nstatus: fail\nexit_code: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$status" >> "$log"
    record_check "$name" "fail" "$log"
    return "$status"
  fi
}

run_logged_in_dir() {
  local name="$1"
  local dir="$2"
  shift 2
  local log="$LOG_DIR/${name}.log"
  {
    printf 'working_dir: %s\n' "$dir"
    printf 'command:'
    printf ' %q' "$@"
    printf '\nstarted_at: %s\n\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } > "$log"
  if (cd "$dir" && "$@") >> "$log" 2>&1; then
    printf '\nfinished_at: %s\nstatus: ok\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$log"
    record_check "$name" "ok" "$log"
  else
    local status=$?
    printf '\nfinished_at: %s\nstatus: fail\nexit_code: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$status" >> "$log"
    record_check "$name" "fail" "$log"
    return "$status"
  fi
}

run_optional() {
  local name="$1"
  shift
  if "$@"; then
    record_check "$name" "ok" ""
  else
    local status=$?
    record_check "$name" "skipped" "exit_code=$status"
    printf '%s: skipped: exit_code=%s\n' "$name" "$status" >> "$SKIP_FILE"
  fi
}

skip_check() {
  local name="$1"
  local reason="$2"
  record_check "$name" "skipped" "$reason"
  printf '%s: skipped: %s\n' "$name" "$reason" >> "$SKIP_FILE"
}

capture_command() {
  local name="$1"
  shift
  local log="$LOG_DIR/${name}.log"
  if command -v "$1" >/dev/null 2>&1; then
    "$@" > "$log" 2>&1 || true
  else
    printf 'skipped: tool not installed: %s\n' "$1" > "$log"
  fi
}

if [[ "$RELEASE_COMMIT" != "$(git rev-parse HEAD)" ]]; then
  git cat-file -e "${RELEASE_COMMIT}^{commit}"
  git -c filter.lfs.smudge= -c filter.lfs.process= -c filter.lfs.required=false checkout --detach "$RELEASE_COMMIT"
fi

{
  git remote -v
} > "$LOG_DIR/git-remotes.log" 2>&1
git status --short > "$LOG_DIR/git-status-short.log" 2>&1
git rev-parse HEAD > "$LOG_DIR/git-rev-parse-head.log" 2>&1
git log --oneline -20 > "$LOG_DIR/git-log-oneline-20.log" 2>&1
capture_command rustc-version rustc --version
capture_command cargo-version cargo --version
uname -a > "$LOG_DIR/uname.log" 2>&1
capture_command hostnamectl hostnamectl
df -h > "$LOG_DIR/df-h.log" 2>&1
free -h > "$LOG_DIR/free-h.log" 2>&1 || true

if [[ "$DOCS_ONLY" == "1" ]]; then
  skip_check "cargo_fmt" "DOCS_ONLY=1"
  skip_check "cargo_test_workspace" "DOCS_ONLY=1"
  skip_check "cargo_clippy_workspace" "DOCS_ONLY=1"
  skip_check "cargo_build_workspace_release" "DOCS_ONLY=1"
elif [[ -z "$CARGO_WORKSPACE_DIR" ]]; then
  run_logged cargo-workspace-missing false
else
  run_logged_in_dir cargo-fmt "$CARGO_WORKSPACE_DIR" cargo fmt --all --check
  run_logged_in_dir cargo-test-workspace "$CARGO_WORKSPACE_DIR" cargo test --workspace
  run_logged_in_dir cargo-clippy-workspace "$CARGO_WORKSPACE_DIR" cargo clippy --workspace --all-targets -- -D warnings
  run_logged_in_dir cargo-build-workspace-release "$CARGO_WORKSPACE_DIR" cargo build --workspace --release
fi

if [[ -f scripts/registry_readiness_check.sh ]]; then
  run_logged registry-readiness-check bash scripts/registry_readiness_check.sh
else
  skip_check "registry_readiness_check" "script missing"
fi

for smoke in \
  scripts/deployment-readiness-smoke.mjs \
  scripts/pilot-validation-smoke.mjs \
  scripts/browser-conformance-smoke.mjs
do
  if [[ -f "$smoke" ]]; then
    if [[ "$smoke" == *browser-conformance-smoke.mjs ]]; then
      skip_check "$(basename "$smoke")" "requires live stand unless explicitly run by operator"
      continue
    fi
    run_logged "$(basename "$smoke" .mjs)" node "$smoke"
  else
    skip_check "$(basename "$smoke")" "script missing"
  fi
done

if [[ -f scripts/validate_install_kit.sh ]]; then
  run_logged validate-install-kit bash scripts/validate_install_kit.sh
else
  skip_check "validate_install_kit" "script missing"
fi

if [[ "$DOCS_ONLY" == "1" ]]; then
  skip_check "cargo_metadata" "DOCS_ONLY=1"
  skip_check "cargo_tree" "DOCS_ONLY=1"
elif [[ -n "$CARGO_WORKSPACE_DIR" ]] && command -v cargo >/dev/null 2>&1; then
  run_logged_in_dir cargo-metadata "$CARGO_WORKSPACE_DIR" cargo metadata --format-version 1 --no-deps
  cp "$LOG_DIR/cargo-metadata.log" "$OUTPUT_DIR/cargo-metadata.json"
  if (cd "$CARGO_WORKSPACE_DIR" && cargo tree --version >/dev/null 2>&1); then
    run_logged_in_dir cargo-tree "$CARGO_WORKSPACE_DIR" cargo tree
    cp "$LOG_DIR/cargo-tree.log" "$OUTPUT_DIR/cargo-tree.txt"
  else
    skip_check "cargo_tree" "cargo tree not available"
  fi
elif [[ -z "$CARGO_WORKSPACE_DIR" ]]; then
  skip_check "cargo_metadata" "Cargo.toml not found"
  skip_check "cargo_tree" "Cargo.toml not found"
else
  skip_check "cargo_metadata" "cargo not installed"
  skip_check "cargo_tree" "cargo not installed"
fi

SOURCE_ARCHIVE="$ARTIFACT_DIR/${RELEASE_VERSION}-source.tar.gz"
git -c filter.lfs.smudge= -c filter.lfs.process= -c filter.lfs.required=false archive --format=tar.gz --output="$SOURCE_ARCHIVE" HEAD
record_check "source_archive" "ok" "$SOURCE_ARCHIVE"

BINARY_ARCHIVE="$ARTIFACT_DIR/${RELEASE_VERSION}-binaries.tar.gz"
if [[ "$DOCS_ONLY" == "1" ]]; then
  printf 'skipped: DOCS_ONLY=1\n' > "$BINARY_ARCHIVE.skip"
  skip_check "binary_archive" "DOCS_ONLY=1"
elif [[ -d target/release ]]; then
  tar -czf "$BINARY_ARCHIVE" target/release
  record_check "binary_archive" "ok" "$BINARY_ARCHIVE"
else
  skip_check "binary_archive" "target/release missing"
fi

if command -v cargo-cyclonedx >/dev/null 2>&1; then
  run_logged sbom-cyclonedx cargo cyclonedx --format json --output-file "$OUTPUT_DIR/sbom-cyclonedx.json"
else
  skip_check "sbom_cyclonedx" "cargo-cyclonedx not installed"
fi

if command -v syft >/dev/null 2>&1; then
  run_logged sbom-syft-cyclonedx syft . -o cyclonedx-json="$OUTPUT_DIR/sbom-syft-cyclonedx.json"
  run_logged sbom-syft-spdx syft . -o spdx-json="$OUTPUT_DIR/sbom-spdx.json"
else
  skip_check "sbom_spdx" "syft not installed"
  skip_check "sbom_syft_cyclonedx" "syft not installed"
fi

GENERATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
BUILD_RUNNER_HOST="$(hostname 2>/dev/null || printf unknown)"
PRIMARY_SOURCE_REPOSITORY="https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus"

python3 - "$OUTPUT_DIR/release-evidence-manifest.json" "$RELEASE_VERSION" "$RELEASE_COMMIT" "$GENERATED_AT" "$BUILD_RUNNER_HOST" "$PRIMARY_SOURCE_REPOSITORY" <<'PY'
import json
import sys
from pathlib import Path

manifest_path, release_version, release_commit, generated_at, build_runner, primary_source = sys.argv[1:]
root = Path(manifest_path).parent
checks = []
checks_file = root / "checks.tsv"
if checks_file.exists():
    for line in checks_file.read_text(encoding="utf-8").splitlines():
        name, status, detail = (line.split("\t") + ["", ""])[:3]
        checks.append({"name": name, "status": status, "detail": detail})

artifacts = []
for path in sorted((root / "artifacts").glob("*")):
    if path.is_file():
        artifacts.append(path.relative_to(root).as_posix())

data = {
    "product": "AWatch-rus",
    "release_version": release_version,
    "release_commit": release_commit,
    "build_runner": build_runner,
    "primary_source_repository": primary_source,
    "github_role": "public_mirror_only",
    "generated_at": generated_at,
    "checks": checks,
    "artifacts": artifacts,
    "docs_only": bool(__import__("os").environ.get("DOCS_ONLY") == "1"),
}
Path(manifest_path).write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY

cat > "$OUTPUT_DIR/RELEASE_EVIDENCE_REPORT_RU.md" <<EOF
# Release evidence report

Product: AWatch-rus

Release version: ${RELEASE_VERSION}

Release commit: ${RELEASE_COMMIT}

Generated at: ${GENERATED_AT}

Build runner: ${BUILD_RUNNER_HOST}

Primary source repository: ${PRIMARY_SOURCE_REPOSITORY}

GitHub role: public mirror only.

Responsible person: [ЗАПОЛНИТЬ ПРАВООБЛАДАТЕЛЕМ]

DOCS_ONLY: ${DOCS_ONLY}

## Checks

See \`checks.tsv\` and \`logs/\`.

## Skipped checks

See \`skipped-checks.txt\`.

## Conservative scope

This report does not claim FSTEC/FSB certification. It does not claim SIEM
replacement. It does not claim DLP replacement. It does not claim ML/LLM-based
detection. It does not claim cloud dependency. It does not claim automatic
remediation. It does not claim completed legal registration in the Russian
software registry.
EOF

(
  cd "$OUTPUT_DIR"
  find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
)

if [[ -f scripts/check_release_evidence.sh ]]; then
  bash scripts/check_release_evidence.sh "$OUTPUT_DIR"
else
  skip_check "release_evidence_check" "script missing"
fi

printf 'release_evidence_dir=%s\n' "$OUTPUT_DIR"
