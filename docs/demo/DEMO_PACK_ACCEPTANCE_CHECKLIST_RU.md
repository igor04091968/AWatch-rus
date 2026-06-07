# Customer Demo Pack: Acceptance Checklist

Чеклист используется перед пилотным показом заказчику.

## Материалы

- [ ] Открывается `docs/DEMO_RUNBOOK_RU.md`.
- [ ] Открывается `docs/PILOT_DEMO_SCENARIO_RU.md`.
- [ ] Открывается сценарий руководителя:
  `docs/demo/DEMO_SCENARIO_EXECUTIVE_RU.md`.
- [ ] Открывается сценарий ИБ:
  `docs/demo/DEMO_SCENARIO_SECURITY_RU.md`.
- [ ] Открывается сценарий расследований:
  `docs/demo/DEMO_SCENARIO_FORENSICS_RU.md`.
- [ ] Открывается пример отчета:
  `docs/DEMO_REPORT_EXAMPLE_RU.md`.
- [ ] Открывается ценностное описание пилота:
  `docs/PILOT_VALUE_PROPOSITION_RU.md`.

## Demo Dataset

- [ ] `docs/fixtures/pilot-v1-demo/demo-seed-data.json` является валидным JSON.
- [ ] Dataset содержит только синтетические идентификаторы.
- [ ] Dataset покрывает нормальную работу.
- [ ] Dataset покрывает снижение активности.
- [ ] Dataset покрывает рост удаленных сессий.
- [ ] Dataset покрывает повышенный UEBA.
- [ ] Dataset покрывает incident candidate.
- [ ] Dataset покрывает низкое покрытие агентами.

## Screenshots

- [ ] `docs/screenshots/01-executive-overview.png` существует и не пустой.
- [ ] `docs/screenshots/02-risk-heatmap.png` существует и не пустой.
- [ ] `docs/screenshots/03-security-view.png` существует и не пустой.
- [ ] `docs/screenshots/04-operations-view.png` существует и не пустой.
- [ ] `docs/screenshots/05-investigation-pack.png` существует и не пустой.
- [ ] `docs/screenshots/06-markdown-report.png` существует и не пустой.
- [ ] `docs/screenshots/07-product-architecture.png` существует и не пустой.

## Запреты

- [ ] В demo-pack нет реальных IP-адресов, hostname, логинов, ФИО и
  подразделений заказчика.
- [ ] Planned/future не описаны как implemented.
- [ ] pfSense readiness описан как `contract_only`, если ingestion отдельно не
  включен и не принят.
- [ ] AWatch-rus не заявляется как SIEM, EDR или классическая DLP.
- [ ] ML/LLM не заявлены и не используются.
