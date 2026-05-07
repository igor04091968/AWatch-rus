# Artifacts Policy

## Purpose

Define which files are source-of-truth and which are generated runtime/research artifacts that must not block or pollute production rollouts.

## Source of Truth

Tracked and reviewable:

- `ansible/`
- `aw-server/`
- `windows/`
- `scripts/`
- `docs/`
- install-kit templates and manifests under `windows/installkit/innosetup/`

## Generated / Volatile Artifacts

Not for production commits:

- `.graphify_*` cache/analysis outputs
- `graphify-out/cache/*`
- `graphify-out/shellcheck-*.txt`
- `graphify-out/validate_dryrun_out*.txt`
- `graphify-out/powershell-parse-results*.json`
- `graphify-out/powershell-pssa-warn-results.json`
- `graphify-out/pssa_diffs.txt`
- `reports/*`
- `tmp/*`

These paths are ignored by `.gitignore` and additionally guarded by `scripts/quality-gate.sh`.

## Rollout Gate

`scripts/prod_rollout.sh` must run only when:

1. `AW_MAINTENANCE_ACK=YES` is set.
2. `scripts/quality-gate.sh` passes.
3. Preflight checks pass:
   - `ansible ping`/`win_ping`
   - `./check-aw-data.sh`
   - `./check-aw-full.sh`

If any gate fails, rollout stops.

## Notes

- Secrets policy remains temporary by operator choice; credentials may still exist in local `inventory.ini` during this phase.
- Dedicated secrets hardening (vault/env-only enforcement) is a separate follow-up track.
