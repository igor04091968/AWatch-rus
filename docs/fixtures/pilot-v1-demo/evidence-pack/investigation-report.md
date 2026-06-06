# Investigation Report

Demo-only investigation report for AWatch-rus Pilot v1.

## Карточка расследования

- Investigation ID: `case-demo-004`
- Candidate ID: `sec-demo-004`
- Status: `in_review`
- Severity: `critical`
- User: `demo-user-005`
- Host: `HOST-DEMO-05`
- App: `admin-tool-demo.exe`
- Network event: `203.0.113.20:443`

## Timeline

| Время UTC | Тип | Сущность | Кратко | Источник |
| --- | --- | --- | --- | --- |
| 2026-06-06T21:40:00Z | activity | `demo-user-005` | Ночная активность в административном приложении | activity_rules |
| 2026-06-06T21:45:00Z | risk | `sec-demo-004` | UEBA severity `critical` | ueba-score-v1 |
| 2026-06-06T21:46:00Z | network | `HOST-DEMO-05` | Сетевой признак по readiness-контракту | pfsense-contract |
| 2026-06-06T21:50:00Z | review | `case-demo-004` | Кейс требует ручной проверки | portal_contract |

## Вывод расследования

Демонстрационный кейс показывает, как AWatch-rus связывает активность,
приложение, хост и сетевой признак в единый пакет проверки. Данные обезличены.
Система не выносит автоматический вердикт и не заявляет production pfSense
ingestion.
