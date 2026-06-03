#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-"$ROOT_DIR/dist/release-v0.2"}"
SBOM_DIR="$OUT_DIR/sbom"
VERSION="release-readiness-v0.2"

mkdir -p "$SBOM_DIR"

cd "$ROOT_DIR"

COMMIT="$(git rev-parse HEAD)"
GENERATED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

cargo metadata --manifest-path adk-rust/Cargo.toml --format-version 1 \
  > "$SBOM_DIR/cargo-metadata-v0.2.json"

(cd adk-rust && cargo tree --workspace) \
  > "$SBOM_DIR/cargo-tree-v0.2.txt"

find aw-server clickhouse-1c detmir-mcp scripts -maxdepth 4 -type f \
  \( -name 'requirements.txt' -o -name 'pyproject.toml' -o -name 'setup.py' \) \
  -print -exec sha256sum {} \; \
  > "$SBOM_DIR/python-inputs-v0.2.txt"

python3 - "$SBOM_DIR/cargo-metadata-v0.2.json" "$SBOM_DIR/cyclonedx-rust-v0.2.json" "$SBOM_DIR/spdx-rust-v0.2.json" "$COMMIT" "$GENERATED_AT" <<'PY'
import json
import sys
from pathlib import Path

metadata_path, cyclonedx_path, spdx_path, commit, generated_at = sys.argv[1:]
metadata = json.loads(Path(metadata_path).read_text(encoding="utf-8"))
packages = sorted(metadata.get("packages", []), key=lambda item: (item.get("name", ""), item.get("version", "")))

components = []
spdx_packages = []
for package in packages:
    name = package.get("name") or "unknown"
    version = package.get("version") or "0"
    license_value = package.get("license") or "NOASSERTION"
    components.append({
        "type": "library",
        "bom-ref": f"pkg:cargo/{name}@{version}",
        "name": name,
        "version": version,
        "licenses": [{"expression": license_value}] if license_value != "NOASSERTION" else [],
        "purl": f"pkg:cargo/{name}@{version}",
    })
    spdx_packages.append({
        "name": name,
        "SPDXID": f"SPDXRef-Package-{name.replace('_', '-').replace('.', '-')}-{version.replace('.', '-')}",
        "versionInfo": version,
        "downloadLocation": "NOASSERTION",
        "filesAnalyzed": False,
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": license_value,
        "copyrightText": "NOASSERTION",
    })

cyclonedx = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.5",
    "serialNumber": f"urn:uuid:00000000-0000-4000-8000-{commit[:12].ljust(12, '0')}",
    "version": 1,
    "metadata": {
        "timestamp": generated_at,
        "component": {
            "type": "application",
            "name": "DetMir AWatch-rus",
            "version": "release-readiness-v0.2",
            "bom-ref": "pkg:generic/detmir-awatch-rus@release-readiness-v0.2",
        },
        "properties": [
            {"name": "git.commit", "value": commit},
            {"name": "scope", "value": "Rust workspace dependencies"},
        ],
    },
    "components": components,
}

spdx = {
    "spdxVersion": "SPDX-2.3",
    "dataLicense": "CC0-1.0",
    "SPDXID": "SPDXRef-DOCUMENT",
    "name": "DetMir AWatch-rus release-readiness-v0.2 Rust SBOM",
    "documentNamespace": f"https://github.com/igor04091968/AWatch-rus/sbom/{commit}",
    "creationInfo": {
        "created": generated_at,
        "creators": ["Tool: scripts/generate_release_sbom_v0_2.sh"],
    },
    "packages": spdx_packages,
}

Path(cyclonedx_path).write_text(json.dumps(cyclonedx, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
Path(spdx_path).write_text(json.dumps(spdx, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY

cat > "$OUT_DIR/RELEASE_ASSETS_MANIFEST-v0.2.json" <<JSON
{
  "release": "$VERSION",
  "generated_at_utc": "$GENERATED_AT",
  "git_commit": "$COMMIT",
  "assets": [
    "sbom/cyclonedx-rust-v0.2.json",
    "sbom/spdx-rust-v0.2.json",
    "sbom/cargo-metadata-v0.2.json",
    "sbom/cargo-tree-v0.2.txt",
    "sbom/python-inputs-v0.2.txt"
  ]
}
JSON

(
  cd "$OUT_DIR"
  find . -type f ! -name 'SHA256SUMS-v0.2.txt' ! -name '*.sig' -print \
    | sort \
    | sed 's#^\./##' \
    | xargs -r sha256sum > SHA256SUMS-v0.2.txt
)

cat <<EOF
release SBOM v0.2 generated
out_dir=$OUT_DIR
commit=$COMMIT
assets:
$(sed 's/^/  /' "$OUT_DIR/SHA256SUMS-v0.2.txt")
EOF
