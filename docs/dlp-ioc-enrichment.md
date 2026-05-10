# DLP IOC Enrichment from Hayabusa/Sigma

This adds a safe offline pipeline to preload DLP blacklists from static Sigma indicators.

## Source

- Sigma rules from Hayabusa ruleset (`hayabusa-rules` YAML files).

## Extracted indicators

- `Image|endswith` -> `process_image_endswith`
- `CommandLine|contains` -> `commandline_contains`
- `OriginalFileName` -> `original_filename`
- `Hashes|SHA256` -> `sha256`

## Scripts

- `scripts/extract_ioc_from_sigma.py` (core extractor)
- `scripts/build_dlp_ioc_from_hayabusa.sh` (wrapper)

## Production (AW server 10.10.10.13)

IOC enrichment is deployed by `ansible/deploy_aw_server.yml` when `aw_dlp_ioc_enabled=true`.

- systemd service: `aw-dlp-ioc-refresh.service`
- systemd timer: `aw-dlp-ioc-refresh.timer`
- refresh interval: `aw_dlp_ioc_refresh_interval` (default `6h`)
- output dir: `/opt/activitywatch/dlp-ioc/output`
- HTTP export via existing AW worktime API (`:5610`):
  - `http://10.10.10.13:5610/dlp-ioc/ioc_blacklist.json`
  - `http://10.10.10.13:5610/dlp-ioc/ioc_blacklist.csv`
  - `http://10.10.10.13:5610/dlp-ioc/ioc_blacklist.sql`

Mandatory post-deploy checks in Ansible:
- `ioc_blacklist.json`
- `ioc_blacklist.csv`
- `ioc_blacklist.sql`

Each file must exist and be non-empty, otherwise deploy fails.

## Run

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
bash scripts/build_dlp_ioc_from_hayabusa.sh
```

Optional custom paths:

```bash
bash scripts/build_dlp_ioc_from_hayabusa.sh \
  /mnt/usb_hdd1/Projects/hayabusa/rules \
  /mnt/usb_hdd2/Projects/ActivityWatch-Russian/data/dlp-ioc
```

## Output artifacts

- `data/dlp-ioc/ioc_blacklist.json`
- `data/dlp-ioc/ioc_blacklist.csv`
- `data/dlp-ioc/ioc_blacklist.sql`

## DLP import mapping

- `process_image_endswith` -> denied process/image list
- `commandline_contains` -> denied command pattern list
- `original_filename` -> suspicious original filename list
- `sha256` -> malware hash blocklist

## Safety notes

- This pipeline only creates export artifacts and does not modify running DLP agents.
- Review and tune false positives before enforcing blocking in production.
