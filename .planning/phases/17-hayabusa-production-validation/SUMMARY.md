# Phase 17 Summary: Production Validation

## Result

Phase 17 is closed.

## What was proven live

A real end-to-end forensic path was executed and closed on production infrastructure:

1. Windows EVTX package was exported on `SHARKON2025`.
2. The real zip package was transferred to `10.10.10.13`.
3. `aw-hayabusa accept` and `aw-hayabusa process-inbox --mode incident` were run through the standard wrapper.
4. Hayabusa generated a real report set with bounded traceability metadata.
5. The result was linked back into AW-rus case management as bounded `forensics.hayabusa` metadata.

## Live proof record

- host: `SHARKON2025`
- case id: `30`
- intake id: `20260521T125653Z_SHARKON2025-phase17-rerun3`
- package path: `/opt/hayabusa/archive/packages/SHARKON2025/20260521T125653Z_SHARKON2025-phase17-rerun3.zip`
- sha256: `e86b9abbfc1d706ac706c6c8a89509ab17023344c50880641e9175f73f1198d4`
- report dir: `/opt/hayabusa/reports/SHARKON2025/20260521T125654Z_incident_20260521T125653Z_SHARKON2025-phase17-rerun3`
- report artifacts:
  - `summary.html`
  - `manifest.json`
  - `run.log`
  - `timeline.jsonl`
  - `logon-summary-successful.csv`
  - `logon-summary-failed.csv`

## Production bugs found and fixed during validation

- `aw-hayabusa` treated `unzip` warning return code `1` as a hard failure for Windows-created zip archives that use backslashes as path separators.
- timeline modes were using the wrong Hayabusa config path; the wrapper must pass `rules/config`, not the rules root.

Both issues were fixed in `aw-server/hayabusa/aw-hayabusa.sh` and retested live against the same package.

## Why this closes the phase

- the path is no longer theoretical or docs-only; it was proven on a real Windows export package
- traceability from host to package to report directory is explicit
- AW-rus case linkage now stores bounded forensic metadata exactly as designed
- the remaining gaps are operational tuning items, not missing core implementation

## Tuning backlog after the live run

- keep at least one preserved sample EVTX zip for future regression reruns
- consider a self-check in `aw-hayabusa doctor` for `rules/config` completeness
- optionally persist a compact machine-readable proof manifest for future audits
