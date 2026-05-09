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

