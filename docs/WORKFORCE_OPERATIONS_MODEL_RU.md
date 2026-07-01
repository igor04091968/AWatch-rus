# Workforce Operations Model

Статус: implemented in Worktime API and DetMir portal.

Модель отвечает на главный управленческий вопрос AWatch-rus Workforce:
рабочая активность сотрудников, загрузка, простои, перегруз и дисциплина
рабочего процесса. Это rule-based слой операционного контроля. Он не является
HR-оценкой, не использует ML/LLM и не выполняет автоматических санкций.

## Где смотреть

Основные точки:

- Worktime API:
  `GET /reports/worktime/management?format=json`;
- Worktime HTML:
  `GET /reports/worktime/management?format=html`;
- DetMir portal:
  `/api/reports`, блок `workforce_operations`;
- UI портала:
  роли `Руководитель` и вкладка `Отчеты`, блок `Операционная загрузка`.

## Источники

Модель использует только подтвержденные рабочие источники:

- ActivityWatch worktime rows;
- bucket рабочих сессий RDP;
- интервалы активности в рабочем окне;
- configured owner/department aliases;
- freshness/coverage metadata, которые уже возвращает Worktime API.

Отсутствие данных не считается простоем. При пропусках источников строка
получает `data_confidence=low` и guardrail
`low_confidence_not_for_discipline`.

## Runtime-настройки

Основной файл политики:

- пример: `configs/worktime-interpretation-policy.example.json`;
- runtime: `/etc/activitywatch/worktime-interpretation-policy.json`;
- env path: `AW_WORKTIME_MANAGER_INTERPRETATION_POLICY`.

Поля policy:

| Поле | Смысл | Рекомендуемое значение |
| --- | --- | --- |
| `underload_threshold` | порог недогруза от рабочего окна | `0.35..0.45` |
| `overload_threshold` | порог перегруза от рабочего окна | `1.10..1.25` |
| `drop_threshold_pct` | порог просадки тренда | `10..25` |
| `night_work_after` | начало вечернего/ночного отклонения | `20:00` |
| `weekend_work` | учитывать выходные отклонения | `true` |
| `min_trend_points` | минимум daily points для тренда | `3..7` |
| `off_hours_threshold_seconds` | минимум внерабочей активности для флага | `1800` |

`underload_threshold` и `overload_threshold` можно задавать дробью или
процентом: `0.45` равно `45`, `1.15` равно `115`.
Для перегруза effective threshold fail-closed зажат в диапазон `100..300`, чтобы
значение ниже 100% не создавало ложный статус перегруза.

Env fallback:

- `AW_WORKTIME_MANAGER_TARGET_COVERAGE_PCT`;
- `AW_WORKTIME_MANAGER_LOW_COVERAGE_PCT`;
- `AW_WORKTIME_MANAGER_OVERLOAD_COVERAGE_PCT`;
- `AW_WORKTIME_MANAGER_TREND_MIN_POINTS`;
- `AW_WORKTIME_MANAGER_TREND_DELTA_PCT`;
- `AW_WORKTIME_MANAGER_OFF_HOURS_THRESHOLD_SECONDS`;
- `AW_WORKTIME_MANAGER_NIGHT_WORK_AFTER`;
- `AW_WORKTIME_MANAGER_WEEKEND_WORK_ENABLED`.

Веса приложений остаются отдельной политикой:

- пример: `configs/detmir-workforce-policy.example.json`;
- runtime: `/etc/detmir-portal-workforce-policy.json`.

Она влияет на explainable KPI и weighted activity, но не подменяет
операционные статусы загрузки/простоя.

## API contract

`/reports/worktime/management?format=json` содержит:

```json
{
  "workday": {
    "target_coverage_pct": 75,
    "low_coverage_pct": 35,
    "overload_coverage_pct": 115
  },
  "workforce_operations": {
    "status": "ATTENTION",
    "summary": {},
    "rows": [],
    "model": {
      "type": "rule_based",
      "ml": false,
      "llm": false,
      "version": "workforce-operations-v1"
    }
  }
}
```

Каждая строка сотрудника содержит:

- `workday_active_seconds`, `workday_active_hhmm`;
- `workday_idle_seconds`, `workday_idle_hhmm`;
- `coverage_pct`;
- `load_status`;
- `idle_status`;
- `discipline_status`;
- `data_confidence`;
- `recommended_action`.

Полный roster в `rows[]` дополнительно содержит `operations`,
`operations.evidence`, `operations.guardrail` и
`operations_recommended_action`.

## Статусы загрузки

| Status | Значение | Действие |
| --- | --- | --- |
| `insufficient_data` | рабочее окно еще не началось или равно нулю | не делать вывод |
| `no_data` | нет сессий или worktime samples | проверить источники |
| `no_activity` | сессия/данные есть, активности в окне нет | проверить присутствие и задачи |
| `underloaded` | ниже low threshold | проверить загрузку и доступ к процессам |
| `below_target` | ниже target threshold | уточнить причину отклонения |
| `normal` | в рабочем диапазоне | наблюдать |
| `overloaded` | выше overload threshold | проверить переработку и риск аврала |

## Статусы простоя

| Status | Значение |
| --- | --- |
| `not_applicable` | нет рабочего окна |
| `unknown` | нет достаточных источников |
| `full_workday_idle_or_absent` | активность в рабочем окне отсутствует |
| `idle_detected` | простой выше порога |
| `no_significant_idle` | существенный простой не найден |

## Дисциплина процесса

`discipline_status` показывает отклонение от рабочего процесса, а не
автоматическое нарушение:

- `ok`;
- `off_hours`;
- `late_start`;
- `early_finish`;
- `multiple_flags`.

Для текущего дня `early_finish` не выставляется до завершения рабочего окна.

## Достоверность

`data_confidence`:

- `high`: есть session samples, worktime samples и active samples;
- `medium`: данных мало или нет active samples;
- `low`: нет сессий/worktime samples или рабочее окно невалидно.

Правило: low confidence строки сначала проверяются как проблема источников.
Их нельзя использовать как персональный дисциплинарный вывод.

## Summary

`workforce_operations.summary` содержит:

- `users_count`;
- `action_required_users`;
- `load.unknown_or_no_data_users`;
- `load.underloaded_users`;
- `load.normal_users`;
- `load.overloaded_users`;
- `idle.idle_users`;
- `discipline.review_users`;
- `confidence.low_users`;
- `confidence.medium_users`;
- `confidence.high_users`;
- `guardrail`.

Summary status:

- `LOW_CONFIDENCE`: нет строк или все строки low confidence;
- `ATTENTION`: есть перегруз, простой или дисциплинарные флаги;
- `WATCH`: есть недогруз, нет данных или low confidence;
- `OK`: отклонений нет.

## UI contract

Портал показывает отдельный блок `Операционная загрузка`:

- сводка: требуют разбора, недогруз, перегруз, простой, дисциплина, low
  confidence;
- таблица сотрудников: active/idle/coverage/load/idle/discipline/confidence;
- рекомендуемое действие;
- guardrail и версию rule-based модели.

Это отдельный блок от `Почему такой индекс активности?`: explainable KPI
отвечает на вопрос "почему такой процент", а Workforce Operations отвечает
"кого и почему нужно разобрать".

## Ограничения

- Не утверждать автоматическую оценку эффективности сотрудника.
- Не считать missing data простоем.
- Не смешивать Security/Forensics claims с Workforce Operations.
- Не заявлять ML/LLM detection.
- Не выполнять автоматическое remediation/action.
- Не использовать GitHub Actions или демо-данные как registry release evidence.

## Проверка после изменения

Минимальный локальный контур:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo fmt --all --check
cargo test -p worktime-api -p detmir-portal --locked
cargo clippy -p worktime-api -p detmir-portal --all-targets --locked -- -D warnings
```

Минимальный live smoke:

```bash
curl -fsS 'http://10.10.10.13:5610/reports/worktime/management?format=json' \
  | jq '.workforce_operations.summary'

curl -fsS 'http://10.10.10.2:8720/api/reports?role=manager' \
  | jq '{status: .workforce_operations.summary.status, rows: (.workforce_operations.rows | length)}'
```

Браузерный smoke: открыть `http://10.10.10.2:8720/`, выбрать представление
менеджера и проверить блок `Операционная загрузка`. В рабочем состоянии должны
быть видны summary-карточки, таблица сотрудников, `workforce-operations-v1` и
guardrail про `low confidence`.
