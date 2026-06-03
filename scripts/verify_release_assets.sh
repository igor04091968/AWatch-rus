#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/verify_release_assets.sh <asset-dir>
  scripts/verify_release_assets.sh --self-test

Checks:
  - SHA256SUMS*.txt exists and verifies all release assets.
  - Each SHA256SUMS*.txt has a detached signature (*.sig).
  - Signature is verified when RELEASE_VERIFY_PUBLIC_KEY is set.
  - Without RELEASE_VERIFY_PUBLIC_KEY, signature presence is still enforced.
EOF
}

fail() {
  echo "verify_release_assets: FAIL: $*" >&2
  exit 1
}

verify_dir() {
  local asset_dir="$1"
  [[ -d "$asset_dir" ]] || fail "asset directory does not exist: $asset_dir"

  local sums
  sums="$(find "$asset_dir" -maxdepth 1 -type f -name 'SHA256SUMS*.txt' | sort | head -n 1)"
  [[ -n "$sums" ]] || fail "SHA256SUMS*.txt not found in $asset_dir"
  [[ -s "$sums" ]] || fail "checksum file is empty: $sums"
  [[ -f "$sums.sig" ]] || fail "detached signature is missing: $sums.sig"

  (cd "$asset_dir" && sha256sum -c "$(basename "$sums")") || fail "checksum verification failed: $sums"

  if [[ -n "${RELEASE_VERIFY_PUBLIC_KEY:-}" ]]; then
    [[ -f "$RELEASE_VERIFY_PUBLIC_KEY" ]] || fail "public key not found: $RELEASE_VERIFY_PUBLIC_KEY"
    openssl dgst -sha256 -verify "$RELEASE_VERIFY_PUBLIC_KEY" -signature "$sums.sig" "$sums" >/dev/null \
      || fail "signature verification failed: $sums.sig"
    echo "verify_release_assets: signature verified with $RELEASE_VERIFY_PUBLIC_KEY"
  else
    echo "verify_release_assets: signature file present; cryptographic verification skipped because RELEASE_VERIFY_PUBLIC_KEY is not set"
  fi

  echo "verify_release_assets: OK ($asset_dir)"
}

self_test() {
  local tmp
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN

  printf 'release asset\n' > "$tmp/asset.txt"
  (cd "$tmp" && sha256sum asset.txt > SHA256SUMS-v0.2.txt)

  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$tmp/private-key.pem" >/dev/null 2>&1
  openssl pkey -in "$tmp/private-key.pem" -pubout -out "$tmp/public-key.pem" >/dev/null 2>&1
  openssl dgst -sha256 -sign "$tmp/private-key.pem" -out "$tmp/SHA256SUMS-v0.2.txt.sig" "$tmp/SHA256SUMS-v0.2.txt"

  RELEASE_VERIFY_PUBLIC_KEY="$tmp/public-key.pem" verify_dir "$tmp"

  cp "$tmp/SHA256SUMS-v0.2.txt" "$tmp/SHA256SUMS-v0.2.txt.good"
  printf 'tampered\n' > "$tmp/asset.txt"
  if (RELEASE_VERIFY_PUBLIC_KEY="$tmp/public-key.pem" verify_dir "$tmp") >/dev/null 2>&1; then
    fail "self-test did not catch checksum mismatch"
  fi
  mv "$tmp/SHA256SUMS-v0.2.txt.good" "$tmp/SHA256SUMS-v0.2.txt"
  printf 'release asset\n' > "$tmp/asset.txt"
  rm -f "$tmp/SHA256SUMS-v0.2.txt.sig"
  if (RELEASE_VERIFY_PUBLIC_KEY="$tmp/public-key.pem" verify_dir "$tmp") >/dev/null 2>&1; then
    fail "self-test did not catch missing signature"
  fi

  echo "verify_release_assets: self-test OK"
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" == "--self-test" ]]; then
  command -v openssl >/dev/null 2>&1 || fail "openssl is required for --self-test"
  self_test
  exit 0
fi

[[ $# -eq 1 ]] || {
  usage >&2
  exit 2
}

verify_dir "$1"
