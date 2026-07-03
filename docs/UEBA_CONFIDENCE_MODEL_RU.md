# UEBA Confidence Model

Документ описывает защитный слой интерпретации UEBA Score v1 в AWatch-rus.

Важно: этот слой не меняет scoring, weights, thresholds или severity. Он
объясняет, насколько можно доверять рассчитанному severity в текущем срезе.

## Зачем нужен слой уверенности

UEBA Score отвечает на вопрос:

```text
Насколько сильна обнаруженная аномалия?
```

Confidence отвечает на другой вопрос:

```text
Насколько достаточно данных, чтобы доверять выводу?
```

Поэтому `critical` не означает автоматически подтвержденный инцидент. При
низкой уверенности корректная трактовка:

```text
Высокая аномалия обнаружена, но требуется ручная проверка данных.
```

## Severity

Severity остается частью UEBA Score v1:

| Score | Severity | Смысл |
| --- | --- | --- |
| `0-14` | `normal` | Существенная аномалия не выявлена |
| `15-39` | `low` | Низкий риск, наблюдение |
| `40-69` | `medium` | Требуется внимание |
| `70-84` | `high` | Требуется ручная проверка |
| `85-100` | `critical` | Срочная ручная проверка |

Severity не подтверждает нарушение само по себе.

## Confidence

Поддерживаемые уровни:

| Confidence | Смысл |
| --- | --- |
| `high` | Данные свежие, покрытие достаточное, сигналы согласованы |
| `medium` | Есть частичные пропуски или ограниченное подтверждение |
| `low` | Покрытие ниже порога, отсутствуют источники или evidence |
| `unknown` | Данных недостаточно для оценки уверенности |

## Confidence Contributors

Модель учитывает шесть факторов:

| Фактор | Что проверяется |
| --- | --- |
| `agent_coverage` | Доля ожидаемых рабочих мест со свежей телеметрией |
| `data_freshness` | Свежесть данных по ожидаемым узлам |
| `telemetry_completeness` | Наличие Worktime, приложений и классификации |
| `evidence_presence` | Наличие evidence metadata или screenshots |
| `history_depth` | Глубина baseline и число samples |
| `signal_consistency` | Есть ли независимые подтверждающие сигналы |

Если хотя бы один критичный contributor находится в `low`, общий confidence
становится `low`. Это сделано намеренно: лучше потребовать ручную проверку,
чем выдать высокий score за подтвержденный инцидент.

## Classification

Classification не заменяет severity. Она показывает, как интерпретировать
severity с учетом confidence.

| Classification | Смысл |
| --- | --- |
| `confirmed_risk` | Риск как сигнал подтвержден достаточным качеством данных |
| `likely_risk` | Риск вероятен, но подтверждение неполное |
| `needs_investigation` | Высокий score есть, но уверенность недостаточна |
| `insufficient_data` | Данных недостаточно даже для уверенной оценки риска |

`confirmed_risk` не означает автоматически подтвержденный ИБ-инцидент, DLP
событие или нарушение сотрудника. Это только подтверждение качества risk signal.

## API

`GET /api/ueba` возвращает дополнительные поля:

```json
{
  "severity": "critical",
  "score": 100,
  "confidence": "low",
  "confidence_score": 0.8,
  "classification": "needs_investigation",
  "classification_reason": "agent_coverage:coverage_below_target",
  "confidence_reasons": [
    "agent_coverage:coverage_below_target"
  ],
  "evidence_status": "not_available"
}
```

Полный объект `risk` также содержит:

- `confidence_level`;
- `classification`;
- `classification_reason`;
- `confidence_reasons`;
- `confidence_contributors`;
- `evidence_status`.

## Risk Narrative

Risk Narrative получает поля:

```json
{
  "confidence": "low",
  "classification": "needs_investigation"
}
```

При `low` или `unknown` confidence Risk Narrative должен говорить о ручной
проверке и полноте данных, а не о подтвержденном нарушении.

## Action Center

Если UEBA confidence низкий или classification равен `needs_investigation`,
Action Center добавляет действие:

```text
Проверить полноту данных
```

Это действие не исправляет данные автоматически и не меняет scoring. Оно
адресует оператору необходимость проверить покрытие, свежесть и completeness
до жестких управленческих выводов.

## Интерпретация для ролей

### Руководитель

Корректно:

```text
Система видит критичную аномалию, но уверенность низкая. Сначала проверяем
полноту данных, затем принимаем управленческое решение.
```

Некорректно:

```text
Critical означает доказанное нарушение.
```

### ИБ

Корректно:

```text
Critical + low confidence = приоритет ручного triage, не подтвержденный incident.
```

Некорректно трактовать так:

```text
Critical UEBA автоматически является DLP/SIEM incident.
```

### Эксплуатация

Корректно:

```text
При low confidence сначала проверяются agent coverage, freshness и missing
telemetry.
```

## Ограничения

- Confidence layer не меняет score, severity, thresholds или weights.
- Confidence layer не подтверждает ИБ-инциденты автоматически.

## Acceptance Interpretation

Для Pilot/Demo Freeze v1 правильная трактовка:

```text
Severity показывает силу аномалии.
Confidence показывает качество данных.
Classification показывает, можно ли делать вывод или нужен ручной разбор.
```
