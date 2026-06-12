# DLP IOC Enrichment from Hayabusa/Sigma

This document describes the automatic DLP IOC/signature replenishment pipeline.
It is separate from the DLP Policy Engine lifecycle and from the Hayabusa EVTX
forensics runner.

## Purpose

The pipeline preloads DLP indicator blacklists from static Sigma indicators. It
is used to enrich endpoint DLP rules without manually editing endpoint JSON
policy files.

## Source

- Upstream ruleset: `Yamato-Security/hayabusa-rules`
- Default source URL:
  `https://github.com/Yamato-Security/hayabusa-rules/archive/refs/heads/main.zip`
- Ansible variable: `aw_dlp_ioc_rules_zip_url`
- Production enable flag: `aw_dlp_ioc_enabled`

Hayabusa rules may carry licenses that are separate from the Hayabusa binary.
Check the upstream ruleset license before packaging or redistributing generated
artifacts.

## Production Pipeline

When `aw_dlp_ioc_enabled=true`, `ansible/deploy_aw_server.yml` installs and
starts this chain on the AW server:

1. `aw-dlp-ioc-refresh.timer` runs on boot and then every
   `aw_dlp_ioc_refresh_interval` (`6h` by default).
2. The timer starts `aw-dlp-ioc-refresh.service`.
3. The service executes `/usr/local/bin/aw-dlp-ioc-refresh.sh`.
4. The wrapper downloads `aw_dlp_ioc_rules_zip_url`.
5. The wrapper unpacks `hayabusa-rules` Sigma YAML files into a temporary
   working directory.
6. The wrapper runs `/usr/local/bin/aw-extract-ioc-from-sigma`.
7. The Rust extractor writes generated IOC artifacts to
   `/opt/activitywatch/dlp-ioc/output`.
8. `aw-worktime-api` serves the artifacts from `/dlp-ioc/...` for DLP policy
   consumption.

Production units:

- `aw-dlp-ioc-refresh.service`
- `aw-dlp-ioc-refresh.timer`

Production paths:

- workdir: `/opt/activitywatch/dlp-ioc`
- output dir: `/opt/activitywatch/dlp-ioc/output`
- latest symlink: `/opt/activitywatch/dlp-ioc/latest`

## Extractor

Primary extractor:

- crate: `adk-rust/crates/extract-ioc-from-sigma`
- installed binary: `/usr/local/bin/aw-extract-ioc-from-sigma`
- local build:

```bash
cd <PROJECT_ROOT>/adk-rust
cargo build --release -p extract-ioc-from-sigma
```

Local/manual wrapper:

- `scripts/build_dlp_ioc_from_hayabusa.sh`

```bash
cd <PROJECT_ROOT>
bash scripts/build_dlp_ioc_from_hayabusa.sh \
  /mnt/usb_hdd1/Projects/hayabusa/rules \
  <PROJECT_ROOT>/data/dlp-ioc
```

The old Python extractor path is not the production path. Do not document
`scripts/extract_ioc_from_sigma.py` as the current core extractor.

## Extracted Indicators

Supported Sigma fields:

- `Image|endswith` -> `process_image_endswith`
- `CommandLine|contains` -> `commandline_contains`
- `OriginalFileName` -> `original_filename`
- `Hashes|SHA256` -> `sha256`

The extractor de-duplicates and sorts rows before writing outputs.

## Output Artifacts

Generated files:

- `ioc_blacklist.json`
- `ioc_blacklist.csv`
- `ioc_blacklist.sql`

Production HTTP export through `aw-worktime-api` (`:5610`):

- `http://<AW_SERVER_HOST>:5610/dlp-ioc/ioc_blacklist.json`
- `http://<AW_SERVER_HOST>:5610/dlp-ioc/ioc_blacklist.csv`
- `http://<AW_SERVER_HOST>:5610/dlp-ioc/ioc_blacklist.sql`

`aw-worktime-api` only serves these three IOC filenames from the DLP IOC
directory.

## Endpoint Consumption

Windows DLP policy consumes the feed through the `ioc` block:

```json
{
  "ioc": {
    "enabled": true,
    "source": "http://aw-server.example.local:5610/dlp-ioc/ioc_blacklist.json",
    "format": "hayabusa_sigma_v1",
    "refreshMinutes": 60
  }
}
```

The endpoint collector loads this source and reports loaded IOC state in its
health/heartbeat data, including `iocRulesLoaded`.

## Operational Checks

Server checks:

```bash
systemctl status aw-dlp-ioc-refresh.timer --no-pager
systemctl status aw-dlp-ioc-refresh.service --no-pager
journalctl -u aw-dlp-ioc-refresh.service -n 80 --no-pager
ls -lh /opt/activitywatch/dlp-ioc/output/ioc_blacklist.*
curl -fsS http://127.0.0.1:5610/dlp-ioc/ioc_blacklist.json | jq 'length'
```

Expected result:

- timer is enabled and active;
- last service run completed successfully;
- `ioc_blacklist.json`, `ioc_blacklist.csv`, and `ioc_blacklist.sql` exist and
  are non-empty;
- Worktime API serves the JSON feed;
- endpoint health shows non-zero `iocRulesLoaded` when the feed contains rules.

Mandatory post-deploy checks in Ansible require all three output files to exist
and be non-empty. Deployment fails if any artifact is missing or empty.

## Boundaries

- This pipeline enriches DLP IOC/signature inputs automatically.
- It does not approve, deploy, or roll back policy versions. That is the role
  of the DLP Policy Engine.
- It does not run Hayabusa against EVTX artifacts. That is the separate
  server-side Hayabusa forensics path.
- It does not modify running DLP agents directly; agents consume the published
  IOC feed through their policy.

Review and tune false positives before using generated indicators for blocking
actions in production.
