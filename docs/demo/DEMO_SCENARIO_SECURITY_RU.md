# Demo Scenario: ИБ

Цель: показать пользу AWatch-rus для ИБ как explainable security analytics
слоя без заявления продукта как SIEM, EDR или классической DLP.

Данные: только синтетический dataset
`docs/fixtures/pilot-v1-demo/demo-seed-data.json` и demo evidence pack.

## Что показывать

- роль `Безопасность`;
- UEBA Score v1;
- incident candidates;
- Risk Narrative;
- `Рекомендуемые действия ИБ`;
- security correlation, если она есть в текущем отчете;
- ограничения pfSense readiness как `contract_only`.

## Порядок показа

1. Выбрать роль `Безопасность`.
2. Показать UEBA Score v1: numeric score, severity и reason codes.
3. Показать кандидата на проверку `sec-demo-004`.
4. Объяснить, что severity является сигналом для ручной проверки, а не
   автоматическим вердиктом.
5. Показать Risk Narrative: как Workforce KPI, UEBA, coverage и candidate
   складываются в общий риск.
6. Показать `Рекомендуемые действия ИБ`: передать кандидата в ИБ, провести
   проверку и зафиксировать решение.
7. При необходимости открыть demo evidence pack:
   `docs/fixtures/pilot-v1-demo/evidence-pack/security-technical-summary.md`.

## Какие выводы делать

- ИБ получает объяснимую очередь проверки без ML/LLM.
- UEBA Score v1 rule-based: причины видны через reason codes.
- Кандидаты отделены от подтвержденных инцидентов.
- pfSense показан честно: contract/readiness layer, не production ingestion.
- Action Center помогает не потерять ручное действие и срок.

## Какие вопросы ожидать

| Вопрос заказчика | Ответ |
| --- | --- |
| Это SIEM-корреляция? | Нет. Это security analytics и readiness-контракты поверх существующих сигналов. |
| UEBA использует ML? | Нет. UEBA Score v1 rule-based, детерминированный и объяснимый. |
| Что значит incident candidate? | Это кандидат на ручную проверку, не подтвержденный инцидент. |
| pfSense уже собирается в production? | Нет. Для Pilot v1 pfSense readiness обозначен как `contract_only`, если ingestion отдельно не включен и не принят. |
| Можно ли автоматически блокировать пользователя? | Нет. Auto-remediation и блокировки не реализуются в этом demo-pack. |

## Границы демонстрации

- Не заявлять DLP/SIEM/EDR функциональность.
- Не обещать автоматическую блокировку, карантин или изменение политик.
- Не показывать реальные сетевые адреса; использовать только TEST-NET примеры.
- Не смешивать ИБ-экран с управленческим Workforce Dashboard.
