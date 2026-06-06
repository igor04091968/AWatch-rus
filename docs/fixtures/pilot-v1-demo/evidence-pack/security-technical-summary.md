# Security Technical Summary

Demo-only security summary for AWatch-rus Pilot v1.

## Сигнал

Кандидат `sec-demo-004` получил severity `critical` из-за комбинации:

- `time_anomaly`;
- `network_anomaly`;
- `history_anomaly`.

## UEBA Score v1

```json
{
  "score": 82,
  "severity": "critical",
  "model": "ueba-score-v1",
  "type": "rule_based",
  "ml_used": false,
  "llm_used": false
}
```

## Network Context

Сетевые признаки показаны на RFC 5737 адресах и относятся к readiness-контракту:

- source host: `HOST-DEMO-05`;
- source IP: `192.0.2.15`;
- destination: `203.0.113.20:443`;
- provider status: `contract_only`.

## Ограничения

- это не production pfSense ingestion;
- это не SIEM-correlation;
- риск является кандидатом на ручную проверку;
- окончательное решение принимает специалист.
