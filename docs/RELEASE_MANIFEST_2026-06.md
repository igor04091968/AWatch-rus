# Release manifest 2026-06

Release tag: `v1.0.1-public-review`

Release commit: Git tag target for `v1.0.1-public-review`.

Build input commit for install-kit/SBOM artifacts:
`e94174a89c0c0723db8e76bdab001ccb6d83181e`.

Generated: `2026-06-03 09:45:52 MSK`

## Scope

This is a public expert-review/source release for DetMir/AWatch-rus. It
contains sanitized source, documentation, install-kit archives, SBOM inputs and
checksums. It does not contain private runtime configuration, live customer
inventory, tokens, passwords, screenshots, runtime databases or evidence.

## Release-readiness v0.1 overlay

Additional readiness artifacts:

- `docs/RELEASE_READINESS_V0.1_RU.md`
- `docs/SBOM_V0.1_RU.md`
- `docs/PORTAL_SCREENSHOTS_RU.md`
- `docs/diagrams/release-readiness-v0.1.md`
- `docs/screenshots/release-v0.1/portal-operator-readiness.png`
- `docs/screenshots/release-v0.1/portal-manager-workforce.png`
- `docs/screenshots/release-v0.1/portal-incidents-evidence.png`
- `docs/screenshots/release-v0.1/portal-reports-risk.png`
- `docs/screenshots/release-v0.1/portal-owner-summary.png`

## Artifacts

```text
33740fc746e009acea46a4ef801c6f08265055c4f832e05c04fcd25575a4db3d  install-kit-awindows-20260427-211240.zip
1506d5851b6a5b70b7bff051c3e1fb0d063429380b66ce36c92c886d3b619509  install-kit-awindows-20260427-211240.tar.gz
2029e22271b8a55f8e534da8669338faa9e9bfca3fc2a04b786281d38a6af7bc  cargo-metadata-2026-06.json
9d4a0f062260fa0a04005c8359a7aff76259ac28dc15804f8815b32d6995e0e0  cargo-tree-2026-06.txt
268c3979584853f1b0c99c748c50d874eaa5491861747d0d0e921603916b2043  python-inputs-2026-06.txt
```

Release asset names:

- `install-kit-awindows-20260427-211240.zip`
- `install-kit-awindows-20260427-211240.tar.gz`
- `SHA256SUMS-2026-06.txt`
- `cargo-metadata-2026-06.json`
- `cargo-tree-2026-06.txt`
- `python-inputs-2026-06.txt`

## Build and validation commands

Rust release build:

```bash
CARGO_TARGET_DIR=/tmp/detmir-release-target cargo build --release --workspace
```

Rust release artifact check:

```bash
CARGO_TARGET_DIR=/tmp/detmir-release-target scripts/check_detmir_rust_release_artifacts.sh
```

Result:

```text
detmir rust release artifacts: OK (/tmp/detmir-release-target/release)
```

Install-kit rebuild and validation:

```bash
CARGO_TARGET_DIR=/tmp/detmir-release-target scripts/rebuild_install_kit.sh
CARGO_TARGET_DIR=/tmp/detmir-release-target scripts/validate_install_kit.sh
```

Result:

```text
MANIFEST complete: 59 files tracked
Archives match: 60 files
validate_install_kit: OK
```

Quality gate:

```bash
CARGO_TARGET_DIR=/tmp/detmir-release-target scripts/quality-gate.sh
```

Result:

```text
quality-gate: OK
```

Install-kit privacy scan:

```bash
PRIVATE_MARKERS_REGEX='<PRIVATE_HOSTNAME>|<PRIVATE_NETWORK>|<PRIVATE_DOMAIN>|<LOCAL_OPERATOR_HOME>|<ROOT_PRIVATE_PATH>'
rg -n "$PRIVATE_MARKERS_REGEX" install-kit-awindows-20260427-211240 || true
```

Result: no matches.

## SBOM inputs

Rust:

```bash
cargo metadata --manifest-path adk-rust/Cargo.toml --format-version 1 \
  > dist/sbom/cargo-metadata-2026-06.json
cd adk-rust
cargo tree --workspace > ../dist/sbom/cargo-tree-2026-06.txt
```

Python dependency input files:

```bash
find aw-server clickhouse-1c detmir-mcp -maxdepth 3 -type f \
  \( -name 'requirements.txt' -o -name 'pyproject.toml' \) \
  -print -exec sha256sum {} \; > dist/sbom/python-inputs-2026-06.txt
```

## Public hygiene status

Release-facing public docs were audited in:

- `docs/RELEASE_AUDIT_2026-06.md`

Expected policy:

- no live hostnames;
- no private IPs/domains;
- no local operator home paths;
- no private root paths;
- no tokens/passwords/private keys;
- live DetMir commercial runtime values remain in ignored/private config.

## Rollback / operational impact

This release does not modify Proxmox, pfSense, AW server, Windows endpoint or
Telegram runtime. It is a source/release-package hardening step.

If a consumer needs the older public install-kit assets, `v1.0.0` remains
available as the baseline release.
