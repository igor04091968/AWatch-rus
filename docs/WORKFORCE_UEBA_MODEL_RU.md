# Workforce / UEBA Model

## Позиционирование

AWatch-rus v0.3 использует Workforce-first подход:

```text
Телеметрия -> Активность -> Риск -> Расследование -> Отчет
```

Это не сертифицированная СЗИ, не промышленная SIEM и не полнофункциональная DLP. Корректная формулировка для v0.3:

```text
UEBA-compatible rule-based risk scoring v1.
```

## Workforce scoring

Расчетные поля:

- `activity_index`;
- `active_today`;
- `department_activity_index`;
- `owner_activity_index`;
- `trend_status`;
- `anomaly_status`;
- `risk_level`.

Базовая proxy-формула:

```text
Индекс активности = active_seconds / planned_seconds * 100
```

Взвешенный индекс использует роли и веса приложений, если задан workforce policy.

## Статусы

- `OK` - состояние в норме;
- `WARN` - требуется внимание;
- `FAIL` - требуется действие.

Каждый риск обязан объяснять:

- что произошло;
- почему это риск;
- что проверить;
- рекомендуемое действие.

## UEBA risk v1

Риск рассчитывается по правилам, а не по скрытой ML-модели.

Поля объяснимости:

- `confidence`;
- `risk_sources`;
- `baseline_status`;
- `policy_version`;
- `calculated_from`;
- `baseline_window_days`;
- `user_baseline_available`;
- `department_baseline_available`;
- `deviation_score`;
- `baseline_samples`.

## Evidence

Evidence повышает достоверность вывода, но само по себе не является фактором риска. Риск должен исходить из события, отклонения, тренда или правила.

## Не реализуется

- перехват содержимого документов;
- запись экрана;
- кейлоггер;
- скрытый агент;
- контентный DLP-анализ;
- автоматическое редактирование инцидентов;
- автоматическое наказание или блокировка сотрудника.
