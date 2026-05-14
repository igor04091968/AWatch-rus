# Hayabusa Source and Packaging Decision 2026-05-14

## Decision

- upstream source of truth: `Yamato-Security/hayabusa`
- fork policy: do not use a fork unless a concrete required patch exists and is documented
- runtime role in this project: `DFIR enrichment`

## Pinned release

- release tag: `v3.9.0`
- server target platform: `x86_64`, `debian 13`
- selected asset:
  - `hayabusa-3.9.0-lin-x64-gnu.zip`
- selected asset URL:
  - `https://github.com/Yamato-Security/hayabusa/releases/download/v3.9.0/hayabusa-3.9.0-lin-x64-gnu.zip`

## Packaging model

- analysis host: `10.10.10.13`
- install root: `/opt/hayabusa`
- versioned release root: `/opt/hayabusa/releases/v3.9.0`
- active symlink target:
  - `/opt/hayabusa/current`
- suggested executable path:
  - `/opt/hayabusa/current/hayabusa`
- suggested wrapper path:
  - `/usr/local/bin/aw-hayabusa`

## Artifact boundaries

- raw incoming EVTX:
  - `/opt/hayabusa/inbox`
- processed EVTX archive:
  - `/opt/hayabusa/archive`
- generated reports:
  - `/opt/hayabusa/reports`
- run logs / metadata:
  - `/opt/hayabusa/state`

These paths are intentionally outside normal ActivityWatch buckets and outside ordinary DLP artifact roots.

## Integrity note

The official release currently does not publish a separate checksum asset in the GitHub release asset list.

Therefore the deployment model should:

1. download the pinned asset URL;
2. calculate `sha256` locally during automation;
3. store the computed value in deployment logs or a local manifest;
4. fail deployment if the downloaded asset name or pinned tag does not match expectations.

## Why this model

- no dependency on an unreviewed fork;
- reproducible server-side installation;
- no attempt to run Hayabusa as a real-time daemon;
- clean separation between `AW-rus` runtime data and forensic artifacts.
