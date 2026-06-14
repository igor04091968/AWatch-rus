#!/usr/bin/env python3
"""Create a GitHub Actions release package from Rust release binaries."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import stat
import tarfile
from datetime import datetime, timezone
from pathlib import Path

SKIP_DIRS = {"deps", "build", "examples", "incremental"}
SKIP_SUFFIXES = {".d", ".rlib", ".rmeta"}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def is_binary(path: Path) -> bool:
    if not path.is_file():
        return False
    if path.name in SKIP_DIRS:
        return False
    if path.suffix in SKIP_SUFFIXES:
        return False
    return bool(path.stat().st_mode & stat.S_IXUSR)


def collect(release_dir: Path) -> list[Path]:
    items = [item for item in sorted(release_dir.iterdir()) if is_binary(item)]
    if not items:
        raise SystemExit(f"No release binaries found in {release_dir}")
    return items


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def write_archive_checksum(archive: Path) -> None:
    write(archive.with_suffix(archive.suffix + ".sha256"), f"{sha256(archive)}  {archive.name}\n")


def create_compatibility_aliases(out_dir: Path, archive: Path) -> None:
    """Create both linux-x86_64 and linux_x86_64 artifact paths.

    Older workflow edits used the underscore form while the target name uses the
    hyphen form. Keeping both names makes the artifact packaging tolerant to
    either path without changing the release contents.
    """
    out_alias = Path(str(out_dir).replace("linux-x86_64", "linux_x86_64"))
    if out_alias != out_dir:
        if out_alias.exists():
            shutil.rmtree(out_alias)
        shutil.copytree(out_dir, out_alias)

    archive_alias = Path(str(archive).replace("linux-x86_64", "linux_x86_64"))
    if archive_alias != archive:
        archive_alias.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(archive, archive_alias)
        write_archive_checksum(archive_alias)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release-dir", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--target", default="linux-x86_64")
    parser.add_argument("--commit", default="unknown")
    parser.add_argument("--ref", default="unknown")
    parser.add_argument("--run-id", default="unknown")
    args = parser.parse_args()

    release_dir = args.release_dir.resolve()
    out_dir = args.out_dir.resolve()
    archive = args.archive.resolve()

    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)

    binaries = collect(release_dir)
    for binary in binaries:
        shutil.copy2(binary, out_dir / binary.name)

    names = [binary.name for binary in binaries]
    write(out_dir / "BINARIES.txt", "\n".join(names) + "\n")

    checksum_lines = []
    manifest_binaries = []
    for name in names:
        packaged = out_dir / name
        digest = sha256(packaged)
        checksum_lines.append(f"{digest}  {name}")
        manifest_binaries.append(
            {"name": name, "size_bytes": packaged.stat().st_size, "sha256": digest}
        )
    write(out_dir / "SHA256SUMS.txt", "\n".join(checksum_lines) + "\n")

    manifest = {
        "project": "AWatch-rus",
        "target": args.target,
        "commit": args.commit,
        "ref": args.ref,
        "run_id": args.run_id,
        "build_time_utc": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "binaries": manifest_binaries,
    }
    write(out_dir / "BUILD_MANIFEST.json", json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")

    archive.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(out_dir, arcname=out_dir.name)
    write_archive_checksum(archive)
    create_compatibility_aliases(out_dir, archive)


if __name__ == "__main__":
    main()
