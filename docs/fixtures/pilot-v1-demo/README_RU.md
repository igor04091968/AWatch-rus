# Pilot v1 Demo Fixtures

Этот каталог содержит воспроизводимые обезличенные материалы для демонстрации
AWatch-rus Pilot v1.

Состав:

- `demo-seed-data.json` - демонстрационный набор сигналов Workforce, Security,
  Forensics, UEBA и pfSense readiness;
- `evidence-pack/executive-summary.md` - краткий управленческий вывод;
- `evidence-pack/security-technical-summary.md` - техническая сводка для ИБ;
- `evidence-pack/investigation-report.md` - Markdown-отчет расследования;
- `evidence-pack/investigation-contract.json` - пример JSON-контракта
  investigation/evidence package.

Границы:

- это не live ingestion;
- это не production data;
- это не новый collector;
- pfSense остается `contract_only/readiness`;
- UEBA Score v1 остается rule-based без ML/LLM.

В demo fixtures запрещены реальные IP-адреса, hostname, логины, ФИО,
подразделения заказчика и реальные события безопасности.
