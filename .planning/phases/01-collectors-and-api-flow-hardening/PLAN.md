# PLAN — Phase 01: collectors-and-api-flow-hardening

## Goal

Deliver stable collector-to-server data flow so activity/worktime pages have consistent data.

## Work Items

1. Validate collector runtime and log rotation behavior.
2. Validate server ingest endpoints and bucket write/read checks.
3. Validate CORS/origin and report link consistency.
4. Add/adjust scripts or runbook checks to detect zero-data regressions early.

## Verification

- Manual and scripted checks show fresh events in target buckets.
- Host activity page reflects real activity (not `0s`) for active sessions.
- No repeating transport errors in collector logs during test window.

## Status

Planned.
