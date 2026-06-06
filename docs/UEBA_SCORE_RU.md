# UEBA Score v1

## Назначение

UEBA Score v1 нужен для приоритизации ручной проверки. Он не блокирует
пользователей, не меняет сетевые правила и не принимает автоматические решения.

Это rule-based модель без ML, LLM и внешних SaaS-зависимостей.

## Формула

```text
Risk Score =
activity anomaly
+ time anomaly
+ application anomaly
+ network anomaly
+ history anomaly
```

Компоненты возвращаются в `/api/ueba` как `score_components`:

- `activity_anomaly` - просадка/аномалия активности, проблемы Worktime.
- `time_anomaly` - ночная активность, работа вне согласованного окна, выходные.
- `application_anomaly` - DLP-lite сигналы и приложения без явного правила.
- `network_anomaly` - сетевой контекст, если он доступен через интеграции.
- `history_anomaly` - открытая очередь проверки и отклонение от baseline.

## Уровни

| Score | Severity | Статус |
| --- | --- | --- |
| 0-14 | `normal` | `OK` |
| 15-39 | `low` | `WARN` |
| 40-69 | `medium` | `WARN` |
| 70-84 | `high` | `FAIL` |
| 85-100 | `critical` | `FAIL` |

## API

`GET /api/ueba` возвращает:

- `score` - число 0-100;
- `severity` - `normal`, `low`, `medium`, `high` или `critical`;
- `score_components` - пять компонент формулы;
- `reason_codes` - коды сработавших правил;
- `explanation` - человекочитаемое объяснение;
- `model.ml_used=false`;
- `model.llm_used=false`.

## Ограничения

UEBA v1 не является SIEM-корреляцией и не является классическим DLP. Это
объяснимый слой ранжирования риска для Workforce Analytics + Security Analytics
+ Forensics.
