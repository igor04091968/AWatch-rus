## Summary

Describe what changed and why.

## Impact

- Runtime impact: `none / changed / not applicable`
- API impact: `none / changed / not applicable`
- UI impact: `none / changed / not applicable`
- Documentation impact: `none / changed / not applicable`
- Rollback impact: `none / documented / not applicable`
- Evidence impact: `none / registry docs updated / release evidence required`

## Validation

List commands executed. Use `skipped: <reason>` when a check requires a live
stand or unavailable tool.

## Review Checklist

- [ ] I checked that this PR does not publish secrets, tokens, passwords,
      private keys, recovery codes or live credentials.
- [ ] I checked that this PR does not publish personal data, real employee data,
      customer logs or customer infrastructure identifiers.
- [ ] I checked registry claims: no completed registry submission, no
      FSTEC/FSB certification claim, no SIEM/DLP replacement claim.
- [ ] I ran relevant checks or documented why a check was skipped.
- [ ] I stated runtime/API/UI impact.
- [ ] I stated documentation impact.
- [ ] I stated smoke-test result or why smoke testing is not applicable.
- [ ] I stated rollback and evidence impact.
- [ ] I checked that GitHub Actions remains public mirror validation only.
- [ ] I checked that registry release evidence still requires the Russian
      build-runner.

## Registry / Public Mirror Scope

- GitHub is public mirror validation only.
- Primary registry release evidence must be produced on the Russian
  build-runner.
- Update `docs/registry/` when registry-readiness behavior or evidence changes.

## Safety

- No secrets, tokens, passwords or private keys.
- No personal data.
- No real employee logs.
- No customer evidence unless anonymized.
- No unsupported claims about certification, DLP/SIEM replacement or legal
  registry completion.
