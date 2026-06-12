# UEBA Critical Evidence Review

Дата проверки: 2026-06-07.

Статус: выполнен ручной разбор текущего `critical` без изменения алгоритма,
весов, thresholds, Risk Narrative и Action Center.

## Executive Summary

Текущий UEBA severity `critical` подтвержден как фактический результат
rule-based scoring, но не подтвержден как доказанный инцидент ИБ.

Классификация:

```text
Needs Investigation
```

Причина: score `100/100` складывается в основном из Workforce/coverage
сигналов, а не из DLP, network, time или application anomaly:

- `activity_anomaly`: `81`;
- `history_anomaly`: `19`;
- `application_anomaly`: `0`;
- `network_anomaly`: `0`;
- `time_anomaly`: `0`.

Главный вывод: текущий `critical` безопаснее трактовать как высокий
операционный риск качества данных и активности, требующий ручной проверки.
Показывать его руководителю как подтвержденное нарушение нельзя.

## Scope

Проверялось:

- live `/portal/api/ueba`;
- live `/portal/api/reports` в разных ролях;
- live `/portal/api/workforce/kpi/explain`;
- live `/portal/api/risk/narrative`;
- live `/portal/api/actions`;
- агрегированная свежесть ActivityWatch buckets;
- состояние portal service и production endpoints;
- согласованность UEBA -> Risk Narrative -> Recommended Actions.

Не выполнялось:

- изменение UEBA score calculation;
- изменение severity thresholds;
- изменение weights;
- отключение правил;
- изменение Risk Narrative;
- изменение Action Center;
- выгрузка сырых событий;
- сохранение реальных пользователей, hostname, IP, логинов, подразделений или
  forensic payload в Git.

## Current Severity

Live UEBA summary:

| Поле | Значение |
| --- | --- |
| Score | `100` |
| Severity | `critical` |
| Status | `FAIL` |
| Model | `rule_based` |
| ML used | `false` |
| LLM used | `false` |
| Policy version | `ueba-rule-v1` |
| Score cap | `100` |

В full report также подтверждено:

- `confidence`: `0.8`;
- `baseline_status`: `per_user_department_baseline_skeleton`;
- `baseline_window_days`: `30`;
- `user_baseline_available`: `true`;
- `department_baseline_available`: не подтверждено;
- baseline samples: `total=40`, `users=19`, `departments=21`.

## Evidence Summary

Обезличенная цепочка evidence:

| Источник | Статус | Наблюдение |
| --- | --- | --- |
| DLP counts | доступен | `warn=0`, `fail=0` |
| Incident queue | доступен | открытые вопросы есть |
| Evidence metadata | доступен | `items=0`, `screenshots=0`; используется только для confidence |
| Workforce insights | доступен | `items=9` |
| Workforce policy audit | доступен | политика доступна |
| UEBA baseline | доступен | baseline samples есть |
| UEBA policy | доступен | ошибка policy loading отсутствует |

Из этого следует:

- текущий `critical` не подкреплен DLP fail/warn;
- текущий `critical` не подкреплен screenshot/evidence package;
- основной источник риска - Workforce insights и baseline deviation;
- security-events агрегат есть, но сам по себе не доказывает инцидент.

## Signal Contributions

Raw rule contributions до score cap:

| Сигнал | Количество | Вес | Raw contribution |
| --- | ---: | ---: | ---: |
| `open_incidents` | 1 | `+15` | `+15` |
| `workforce_drop` | 8 | `+15` | `+120` |
| `workforce_anomaly` | 1 | `+10` | `+10` |
| `baseline_deviation` | 1 | `+15` | `+15` |

Raw total:

```text
15 + 120 + 10 + 15 = 160
```

Final score после cap:

```text
min(160, 100) = 100
```

Компоненты после пересчета capped score:

| Component | Score |
| --- | ---: |
| `activity_anomaly` | `81` |
| `history_anomaly` | `19` |
| `application_anomaly` | `0` |
| `network_anomaly` | `0` |
| `time_anomaly` | `0` |

Важное наблюдение: повторяющиеся `workforce_drop` являются главным драйвером
`critical`. Это может быть реальной массовой просадкой активности, но при
нулевом agent coverage также может быть следствием деградации источников.

## Coverage Assessment

Explainable KPI:

| Поле | Значение |
| --- | --- |
| KPI score | `0` |
| Confidence | `low` |
| Agent coverage | `0%` |
| Data freshness | `fresh` |
| Missing sources | `agent_coverage`, `applications` |

Agent coverage SLA в admin scope:

| Поле | Значение |
| --- | ---: |
| Expected nodes | `1` |
| Reporting nodes 24h | `0` |
| Stale nodes | `1` |
| Missing nodes | `0` |
| Coverage | `0%` |
| Freshness | `0%` |
| SLA status | `CRITICAL` |

ActivityWatch buckets aggregate:

| Показатель | Значение |
| --- | ---: |
| Total buckets | `27` |
| Buckets with metadata end | `22` |
| Fresh within 15 minutes | `9` |
| Fresh within 1 hour | `9` |
| Fresh within 24 hours | `11` |
| Old or missing metadata | `16` |

Оценка покрытия: проблемы покрытия могли искусственно усилить severity. При
`agent_coverage=0%`, `confidence=low` и missing `applications` нельзя
отделить реальную просадку активности от недостатка данных без ручной проверки.

## Explainability Consistency

UEBA, Risk Narrative и Action Center в целом согласованы:

- UEBA показывает `critical` из-за Workforce и baseline/history signals;
- Risk Narrative показывает `critical` и указывает на низкий KPI, низкое
  доверие к KPI, низкое покрытие агентов, baseline/security context и
  кандидатов на проверку;
- Action Center рекомендует проверить агентов, назначить владельца действий и
  проверить подразделение/активность.

Найденные ограничения согласованности:

1. `/portal/api/ueba` в standalone response не показывает full explainability
   поля `confidence`, `baseline_*`, `risk_sources`; они доступны в
   `/portal/api/reports`.
2. Risk Narrative показывает candidates в executive/admin context, а security
   role получает другой scope: security correlation есть, но executive
   candidate list скрыт. Это похоже на role filtering, а не на runtime bug.
3. Risk Narrative включает агрегированные security events, но UEBA components
   показывают `application_anomaly=0` и `network_anomaly=0`; значит security
   events не должны трактоваться как доказанная причина `critical`.

Противоречий, требующих немедленного изменения алгоритма, не выявлено.

## False Positive Analysis

Признаки возможного true positive:

- много Workforce signals;
- baseline deviation есть;
- открытая очередь проверки есть;
- Risk Narrative и Action Center согласованно поднимают приоритет.

Признаки возможного false positive / data-quality noise:

- agent coverage `0%`;
- freshness SLA `0%`;
- missing sources: `agent_coverage`, `applications`;
- KPI confidence `low`;
- DLP warn/fail отсутствуют;
- evidence screenshots отсутствуют;
- network/time/application components равны `0`;
- score достигает `critical` за счет повторяющихся однотипных
  `workforce_drop`.

Вывод: текущих данных недостаточно для True Positive. Также недостаточно
данных, чтобы назвать это False Positive. Корректная классификация -
`Needs Investigation`.

## Security Interpretation

Текущий `critical` нельзя считать подтвержденным security incident.

Статус для ИБ:

```text
Operational Risk: confirmed
Security Risk: unknown
```

Что можно утверждать:

- есть критичный операционный риск качества данных и интерпретации Workforce
  KPI;
- есть очередь ручной проверки;
- есть baseline/workforce отклонения.

Что нельзя утверждать:

- подтвержденная утечка;
- подтвержденный DLP incident;
- подтвержденная сетевой атакой anomaly;
- подтвержденное нарушение конкретного пользователя или подразделения.

## Executive Interpretation

Executive readiness:

```text
Insufficient Confidence
```

Текущий `critical` можно показывать руководителю только как пример:

```text
Система обнаружила критичный риск, но перед управленческим выводом требуется
проверить покрытие агентов и подтвердить первичные данные.
```

Нельзя показывать как:

```text
Система доказала нарушение / инцидент / виновника.
```

Для демо руководителю безопасная формулировка:

> Главный риск сейчас - не доказанное нарушение, а недостаточная достоверность
> данных при множественных сигналах просадки активности. Следующее действие -
> проверить покрытие агентов и передать кандидаты на ручной разбор.

## Classification

Итоговая классификация:

```text
Needs Investigation
```

Не `True Positive`, потому что нет достаточного evidence для подтверждения
инцидента: DLP/network/time/application components равны `0`, screenshots/evidence
отсутствуют, agent coverage `0%`.

Не `False Positive`, потому что Workforce/baseline/open-review signals реально
сработали и runtime ошибок portal service не показал.

## Risks

1. Руководитель может воспринять `critical` как доказанный инцидент, если не
   пояснить низкую confidence/coverage.
2. Нулевое покрытие агентов может искусственно снижать KPI и усиливать
   workforce_drop signals.
3. Повторяющиеся `workforce_drop` могут доминировать score до cap `100`.
4. Высокий агрегат security events без DLP/network contribution может выглядеть
   как ИБ-доказательство, хотя это только контекст.
5. Standalone `/api/ueba` менее объясним, чем full report, потому что не
   возвращает full baseline/confidence metadata.

## Recommendations

До расширения пилота:

1. Проверить агент на ожидаемом рабочем месте: почему expected node есть, но
   reporting nodes за 24 часа `0`.
2. Проверить, почему Explainable KPI видит missing `agent_coverage` и
   `applications`.
3. Проверить freshness по active worktime/window/application buckets без
   раскрытия пользователей и hostname.
4. Передать ИБ только обезличенную очередь signals: `open_incidents`,
   `workforce_drop`, `baseline_deviation`; не заявлять подтвержденный инцидент.
5. На демо использовать wording `требует ручной проверки`, а не
   `подтверждено нарушение`.
6. Отдельно рассмотреть product recommendation: standalone `/api/ueba` может
   возвращать больше explainability metadata, уже присутствующей в report. Это
   recommendation, не изменение в рамках TASK_015.

Не делать в TASK_015:

- не менять weights;
- не менять thresholds;
- не подавлять `workforce_drop`;
- не снижать score вручную;
- не отключать `critical`;
- не менять Risk Narrative или Action Center.

## Conclusion

UEBA `critical` на live-контуре является корректно рассчитанным rule-based
результатом текущих входных сигналов, но не является подтвержденным security
incident.

Финальный статус:

```text
Classification: Needs Investigation
Executive readiness: Insufficient Confidence
Security interpretation: Operational Risk confirmed; Security Risk unknown
Algorithm changes: none
Scoring changes: none
Sensitive data committed: none
```
