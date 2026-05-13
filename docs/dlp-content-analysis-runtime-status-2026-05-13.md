# DLP Content Analysis Runtime Status 2026-05-13

This document records the production-verified state of advanced content analysis on `10.10.10.13`.

## What is live

- Endpoint-side dictionary and regex matching is active in `windows/dlp-endpoint-signals-collector.ps1`.
- Active policy supports:
  - `contentAnalysis.dictionaryPack`
  - `contentAnalysis.regexPack`
  - `contentAnalysis.ocrEnabled`
  - `ioc.*`
- Historical incidents in `aw-dlp-incidents_SHARKON2025` already contain enriched fields:
  - `dictionaryMatches`
  - `regexMatches`
  - `ocrRequested`
- IOC refresh pipeline is deployed and active:
  - `aw-dlp-ioc-refresh.timer`
  - output artifacts:
    - `/opt/activitywatch/dlp-ioc/output/ioc_blacklist.json`
    - `/opt/activitywatch/dlp-ioc/output/ioc_blacklist.csv`
    - `/opt/activitywatch/dlp-ioc/output/ioc_blacklist.sql`

## What was fixed in this phase

- Server-side analyzer dependencies were installed only inside a virtualenv, but there was no canonical wrapper to run the analyzer in production.
- Added `/usr/local/bin/aw-dlp-content-analyzer`, which executes:
  - `/opt/activitywatch/dlp-content-analysis/.venv/bin/python`
  - `/opt/activitywatch/dlp-content-analysis/content_analyzer.py`

## Supported production mode

### Fully supported now

- Endpoint-side enrichment:
  - clipboard and print content are matched against dictionary and regex packs on the endpoint;
  - enriched incidents are sent to AW with structured matches;
  - `ocrRequested=true` is carried into incident metadata when policy requires screenshot/OCR follow-up.
- IOC enrichment:
  - Hayabusa/Sigma-derived IOC artifacts are refreshed on the server and exposed over HTTP for policy consumption.
- Server-side manual/operational analysis:
  - operators can run `aw-dlp-content-analyzer` for text or image artifacts using the deployed packs and OCR stack.

### Not a continuous background pipeline yet

- There is no standalone daemon that automatically scans screenshot artifacts after incident creation.
- OCR is production-usable as a server-side utility path, not as an always-on post-processing service.

## Live verification commands

```bash
sudo systemctl status aw-dlp-ioc-refresh.timer --no-pager
ls -1 /opt/activitywatch/dlp-ioc/output
aw-dlp-content-analyzer --text "СНИЛС 112-233-445 95 пароль qwerty" --dictionary-pack 152-fz-pdn --regex-pack secrets
```

Expected result:

- IOC artifacts exist and are non-empty.
- The analyzer returns dictionary and regex matches for the sample text.
