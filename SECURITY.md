# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately to the project maintainer
before publishing technical details. If a private contact channel is not
available, open a GitHub issue with a minimal description and no exploit,
secret, customer data, employee logs or personal data.

Do not include:

- passwords, tokens, cookies or private keys;
- real employee logs;
- personal data;
- private network details;
- customer evidence that has not been anonymized.

Use demo or anonymized evidence whenever possible.

## Security scope

AWatch-rus is not positioned as a certified security product. It is not a replacement for DLP or SIEM platforms. Public security checks are advisory validation for engineering transparency.

## Public validation

GitHub Actions security checks run in the public mirror:

- cargo audit;
- cargo deny;
- secret-pattern check;
- dependency review for pull requests.

GitHub remains public mirror validation only. Registry release security
evidence must be produced in the Russian build contour on the Russian
build-runner.

## Registry-readiness note

Security checks do not confirm legal completion of Russian software registry
registration. Final submission requires rightsholder confirmation and legal
review.
