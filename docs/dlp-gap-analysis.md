# DLP gap analysis: AWatch-rus vs enterprise DLP class

## Текущий контур AWatch-rus

- Endpoint activity tracking (`aw-watcher-afk`, `aw-watcher-window`).
- Browser URL/domain collection (native UIAutomation collector).
- Rule-based категоризация web-активности.
- Phase-1 DLP policy: rule match + incident bucket `aw-dlp-incidents_<host>` + локальный incident log.
- Автоматизированный deployment (PowerShell, Ansible, Proxmox).

## Разрыв до enterprise DLP уровня

1. **Каналы перехвата**: почта, USB/MTP, печать, clipboard, мессенджеры, облака, file transfer.
2. **Контент-анализ**: PII/dictionaries/EDM/IDM, advanced OCR, document fingerprinting.
3. **Реагирование**: block/quarantine/workflow approvals, исключения, эскалации.
4. **Расследования**: case-management, evidence chain, immutable audit.
5. **Управление**: RBAC/SoD, policy lifecycle, multi-tenant admin model.
6. **Интеграции**: SIEM/SOAR/ITSM, AD/IdP, ticketing.

## Реалистичный roadmap

### Phase 1 (сделано)

- DLP policy JSON + rules.
- Incident generation в отдельный AW bucket.
- Incident cooldown/dedup.

### Phase 2 (внедрено частично)

- USB/print/clipboard collectors (endpoint signals) — внедрено.
- Incident pipeline расширен на endpoint события — внедрено.
- File-operation telemetry (create/delete/rename/archive hints) — прототип внедрён (`windows/file-operations-collector.ps1`).
- Central incident aggregation/export — прототип внедрён (`scripts/aggregate_dlp_events.py`, `docs/dlp-aggregator.md`).

### Phase 3

- Policy engine service (server-side), versioned policies, approval workflow.
- Correlation engine (user + channel + object + time).
- SIEM connector (CEF/JSON over syslog/HTTP).

### Phase 4

- Advanced detectors (dictionary packs, regex packs, OCR pipeline).
- Risk scoring / UEBA.
- Compliance reports (152-ФЗ / PCI DSS / ISO 27001-aligned evidence views).

## Reference links (product capability benchmark)

- https://www.infowatch.ru/products/dlp-sistema-traffic-monitor/vozmozhnosti-dlp-sistemy
- https://www.infowatch.ru/products/dlp-sistema-traffic-monitor/sistemnye-trebovaniya-dlp
- https://www.infowatch.ru/company/presscenter/news/zapatentovana-tekhnologiya-dlya-raspoznavaniya-teksta-na-izobrazheniyakh
