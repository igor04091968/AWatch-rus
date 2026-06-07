## Цель

Устранить риск неправильной интерпретации UEBA Score.

Не менять:

* UEBA scoring;
* UEBA weights;
* UEBA thresholds;
* Risk Narrative scoring;
* Action Center scoring.

Добавить слой уверенности (confidence layer) и защиту от ложной интерпретации severity.

---

## Контекст

По результатам:

```text
TASK_015_UEBA_CRITICAL_EVIDENCE_REVIEW
```

установлено:

```text
UEBA Score = 100

Classification = Needs Investigation

Executive Readiness = Insufficient Confidence

Security Interpretation = Operational Risk Confirmed

Security Incident = Not Confirmed
```

Причина:

```text
activity_anomaly = 81
history_anomaly = 19

coverage = 0%
confidence = low
```

При этом текущий UI может визуально восприниматься как:

```text
Critical = подтвержденный инцидент
```

что неверно.

---

## Основная идея

Разделить:

```text
Severity
```

и

```text
Confidence
```

Severity отвечает:

```text
Насколько сильна аномалия
```

Confidence отвечает:

```text
Насколько мы уверены в выводе
```

---

## Что реализовать

### 1. UEBA Confidence Model

Добавить модель:

```json
{
  "severity": "critical",
  "score": 100,
  "confidence": "low",
  "classification": "needs_investigation",
  "reason": "coverage_below_threshold"
}
```

---

### 2. Confidence Levels

Поддержать:

```text
high
medium
low
unknown
```

---

### 3. Confidence Contributors

Минимальные факторы:

```text
agent_coverage

data_freshness

telemetry_completeness

evidence_presence

history_depth

signal_consistency
```

---

### 4. Confidence Rules

Пример логики:

#### HIGH

```text
coverage >= target

fresh data

multiple corroborating signals

evidence exists
```

#### MEDIUM

```text
partial coverage

some missing telemetry

limited evidence
```

#### LOW

```text
coverage below threshold

missing telemetry

missing evidence

conflicting signals
```

#### UNKNOWN

```text
insufficient data
```

---

### 5. Classification Layer

Добавить:

```text
confirmed_risk

likely_risk

needs_investigation

insufficient_data
```

Важно:

classification не заменяет severity.

---

### 6. Executive View

В Executive Portal показать:

Пример:

```text
UEBA Score: 100

Severity: Critical

Confidence: Low

Classification:
Needs Investigation
```

Добавить пояснение:

```text
Высокая аномалия обнаружена,
но данных недостаточно для подтверждения риска.
```

---

### 7. Security View

В Security Portal показать:

```text
Severity

Confidence

Classification

Evidence Status
```

Пример:

```text
Evidence:
Not Available
```

или

```text
Evidence:
Available
```

---

### 8. Risk Narrative Integration

Обновить Risk Narrative.

Добавить:

```json
{
  "confidence": "low",
  "classification": "needs_investigation"
}
```

---

### 9. Executive Action Center Integration

Если:

```text
confidence = low
```

добавлять действие:

```text
Проверить полноту данных
```

до формирования жестких выводов.

---

### 10. API

Расширить:

```http
GET /api/ueba
```

если контракт позволяет.

Добавить поля:

```json
{
  "confidence": "...",
  "classification": "...",
  "confidence_reasons": []
}
```

Также обновить:

```http
GET /api/risk/narrative
```

при необходимости.

---

### 11. Markdown Reports

Добавить раздел:

```text
UEBA Confidence
```

Показывать:

* severity;
* confidence;
* classification;
* confidence reasons.

---

### 12. OpenAPI / TypeScript

Обновить контракты.

Только если реально изменяются API ответы.

---

### 13. Documentation

Создать:

```text
docs/UEBA_CONFIDENCE_MODEL_RU.md
```

Описать:

* что такое severity;
* что такое confidence;
* что такое classification;
* почему они отличаются;
* примеры интерпретации.

---

## Что запрещено

Запрещено:

* менять UEBA score;
* менять weights;
* менять thresholds;
* менять severity rules;
* скрывать высокий score;
* искусственно занижать риск;
* автоматически подтверждать инцидент;
* добавлять ML;
* добавлять LLM;
* добавлять DLP claims;
* добавлять SIEM claims.

---

## Проверки

Выполнить:

```bash
cargo fmt --all --check

cargo clippy --all-targets --all-features -- -D warnings

cargo test --all

cargo build --release

node scripts/deployment-readiness-smoke.mjs

node scripts/pilot-validation-smoke.mjs

AWATCH_PORTAL_SMOKE_URL=http://127.0.0.1:8720 node scripts/awatch-production-hardening-smoke.mjs

AWATCH_BROWSER_SMOKE_URL=http://127.0.0.1:8720/portal/ node scripts/browser-conformance-smoke.mjs
```

Также:

```bash
git diff --check
```

и sensitive scan.

---

## Критерии приемки

Задача выполнена если:

* severity и confidence разделены;
* classification добавлен;
* Executive UI показывает confidence;
* Security UI показывает confidence;
* Risk Narrative учитывает confidence;
* Action Center учитывает confidence;
* документация создана;
* OpenAPI/TypeScript обновлены при необходимости;
* все проверки проходят;
* UEBA scoring не изменен.

---

## Финальный отчет Codex должен содержать

1. Какие модели добавлены.
2. Какие поля API изменены.
3. Как рассчитывается confidence.
4. Как рассчитывается classification.
5. Изменения UI.
6. Изменения reports.
7. Документация.
8. Результаты проверок.
9. Подтверждение, что scoring/weights/thresholds не менялись.

---

## Выполнение

Дата выполнения: 2026-06-07.

Статус: выполнено.

Добавлено:

* UEBA confidence layer;
* confidence contributors:
  * `agent_coverage`;
  * `data_freshness`;
  * `telemetry_completeness`;
  * `evidence_presence`;
  * `history_depth`;
  * `signal_consistency`;
* classification layer:
  * `confirmed_risk`;
  * `likely_risk`;
  * `needs_investigation`;
  * `insufficient_data`;
* поля `/api/ueba`:
  * `confidence`;
  * `confidence_score`;
  * `classification`;
  * `classification_reason`;
  * `confidence_reasons`;
  * `confidence_contributors`;
  * `evidence_status`;
* поля Risk Narrative:
  * `confidence`;
  * `classification`;
* Action Center guardrail:
  * `Проверить полноту данных` при low/unknown UEBA confidence или
    `needs_investigation`;
* Markdown section:
  * `UEBA Confidence`;
* документация:
  * `docs/UEBA_CONFIDENCE_MODEL_RU.md`.

Не менялось:

* UEBA score calculation;
* UEBA weights;
* UEBA thresholds;
* severity rules;
* Risk Narrative scoring;
* Action Center scoring;
* ML/LLM/DLP/SIEM claims не добавлялись.

Ключевая интерпретация:

```text
Severity = сила аномалии
Confidence = качество данных для вывода
Classification = как трактовать severity с учетом confidence
```

Для случая `critical + low confidence` результат:

```text
classification = needs_investigation
```

Это защищает от неверной трактовки `critical` как автоматически подтвержденного
ИБ-инцидента.
