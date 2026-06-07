# Registry Functional Scope

Документ разделяет текущий функциональный состав AWatch-rus на core, optional
addons и not claimed.

## Core

Core-функции, которые можно описывать как текущий функциональный контур:

- Workforce KPI;
- Explainable KPI;
- UEBA Score v1;
- Risk Narrative;
- Executive Action Center;
- Reports and Markdown export;
- Role-based portals:
  - `executive`;
  - `manager`;
  - `security`;
  - `forensics`;
  - `admin`;
- Rust Agent baseline;
- API contracts;
- Demo Pack;
- readiness and smoke checks.

## Workforce Core

Workforce layer:

- activity index;
- department and owner comparison where data is available;
- explainability factors;
- data coverage and confidence;
- remote session/activity signals where available;
- reports for управленческий контур.

Это не автоматическая HR-оценка сотрудника.

## Security Core

Security layer:

- UEBA Score v1;
- severity levels;
- incident candidates;
- risk reasons;
- security-oriented role filtering;
- pfSense readiness visibility as optional/contract-only layer.

Это не полноценная SIEM и не сертифицированная DLP.

## Forensics Core

Forensics layer:

- investigation cards;
- timeline;
- evidence package;
- Markdown report;
- context linking for `user / host / app / network event` where available.

Это не юридическая экспертиза и не автоматическое доказательство нарушения.

## Optional Addons

Optional addons and deployment-specific directions:

- pfSense;
- 1C;
- AD/LDAP;
- SIEM/syslog;
- external storage;
- Grafana/Prometheus/InfluxDB where included in конкретной поставке;
- ClickHouse where configured for analytics/event storage.

Optional означает, что компонент не является обязательной зависимостью core.

## Not Claimed

AWatch-rus не заявляет:

- полноценную DLP;
- полноценную SIEM;
- EDR/XDR;
- ML/LLM scoring;
- auto-remediation без ручного контроля;
- юридически гарантированное доказательное хранилище;
- обязательный pfSense ingestion;
- готовность future React/Tauri UI как текущий функционал.

## Registry Boundary

Для registry-readiness рекомендуется позиционировать AWatch-rus как:

```text
Workforce-first платформа операционного контроля, технического аудита,
управленческой аналитики, поддержки ИБ-проверок и внутренних расследований.
```

Security и Forensics описываются как прикладные слои продукта, а не как
сертифицированная система защиты информации.
