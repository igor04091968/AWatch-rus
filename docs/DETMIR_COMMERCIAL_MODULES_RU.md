# Коммерческие модули DetMir

DetMir / AWatch-rus коммерчески правильнее подавать как платформу
операционного контроля с тремя прикладными модулями. Первый модуль - Workforce:
он дает ежедневную бизнес-ценность владельцу и руководителю. Security и
Forensics усиливают продукт, но не должны перетягивать позиционирование в
сторону сертифицированной DLP/SIEM/СЗИ.

## DetMir Workforce

Для владельца, директора, руководителя подразделения и операционного менеджера.

Показывает:

- кто реально работал в рабочем окне;
- кто перегружен, простаивает или выпадает из нормального профиля активности;
- сколько времени команда проводит в RDP, 1С и рабочих приложениях;
- какие приложения и процессы забирают рабочее время;
- как меняется загрузка сотрудников и подразделений;
- где тормозят бизнес-процессы.

Основные KPI:

- UEBA риск: read-only `risk_score/risk_level/reasons` для приоритизации
  проверки;
- индекс активности: proxy `активное время / плановое рабочее время`;
- взвешенная активность: только при настроенной role/application policy;
- сравнение подразделений за текущий день;
- сравнение ответственных/владельцев процессов за текущий день;
- статус тренда: `daily_only`, `weekly_ready` или `monthly_ready`;
- активное время;
- простой;
- число рабочих сессий;
- активные приложения;
- документы/операции 1С при наличии связанного бизнес-слоя.

Важно: термин "полезная активность" нельзя использовать для простого proxy по
времени. Он допустим только для будущего/настроенного слоя, где приложения
взвешены по ролям: например, для бухгалтера `1C` имеет высокий вес, а для
маркетолога браузер и соцсети могут быть частью рабочей активности.

### Настройка весов приложений

Публичный пример:

- `configs/detmir-workforce-policy.example.json`

Runtime-файл на сервере портала:

- `/etc/detmir-portal-workforce-policy.json`

Правило:

- `default_role` задает роль для агрегированного отчета;
- `planned_hours_per_day` задает плановый рабочий день для роли;
- `default_weight` применяется к приложениям без явного правила;
- `application_weights` задает веса приложений по подстроке имени;
- `description` объясняет бизнес-смысл роли и используется для прозрачной
  интерпретации weighted KPI.

Пример логики:

- бухгалтер: `1C = 1.0`, `Excel = 0.9`, `YouTube = 0.0`;
- оператор: `1C/CRM/RDP = 0.9..1.0`, офис и почта ниже;
- разработчик: `IDE/code = 1.0`, `terminal = 0.9`, `browser = 0.7`;
- администратор: `RDP/SSH/terminal/monitoring = 0.9..1.0`;
- менеджер/продажи: `CRM/mail/browser/documents = 0.7..1.0`.

JSON отчета содержит объяснение расчета:

- активная роль;
- доступные роли;
- формула `index = weighted_seconds / planned_seconds × 100`;
- плановое время;
- фактическое app time;
- взвешенное время;
- matched rule по каждому приложению из top breakdown.
- audit-блок policy: какие приложения попали под `default_weight`;
- drill-down по сотрудникам: формула, active/plan seconds, индекс и причина.

В портале это раскрывается в экране `Почему такой индекс?`: собственник видит
роль, итоговый weighted KPI, план/app/weighted time и top-12 приложений с весом
и вкладом каждого приложения. Та же секция добавляется в Markdown export
оперативного отчета, чтобы расчет можно было приложить к письму, PDF или
коммерческому отчету без ручного пересказа GUI.

PDF-экспорт выполняется штатной печатью браузера из вкладки `Отчеты`
(`Печать / PDF`). Печатный CSS скрывает навигацию и оставляет отчетные секции,
включая `Почему такой индекс?`, audit `default_weight` и drill-down по
сотрудникам.

Ограничение текущего contract: per-user индекс объясняется по персональному
`active_seconds / planned_seconds`; per-user app-weight breakdown пока не
доступен в worktime payload. Поэтому веса приложений и аудит `default_weight`
являются portfolio-level объяснением, а не персональной раскладкой приложений.
Это честнее, чем выводить ложную детализацию.

Для коммерческих демо, экспертных проверок и внешних PDF/Markdown материалов
используется режим обезличивания:

- `GET /api/reports?anonymize=1`;
- `GET /api/workforce/policy/explain?anonymize=1`;
- кнопка `Демо без имен` во вкладке `Отчеты`.

В этом режиме `employee_details[].user` и `employee_details[].user_id`
заменяются на `Сотрудник N` и `EMPLOYEE-N`. Live-режим без query-флага
сохраняет реальные имена для внутреннего коммерческого контура DetMir.

### UEBA-compatible rule-based risk scoring v1

DetMir Workforce/Security формирует read-only UEBA-compatible rule-based score
для руководителя и ИБ:

- `risk_score`: сумма reason points, capped at 100;
- `risk_level`: `normal`, `low`, `medium`, `high`;
- `confidence`: доверие к расчету; evidence и screenshot повышают confidence,
  но не добавляют risk score сами по себе;
- `risk_sources`: типы источников, которые дали risk reasons;
- `baseline_status`: статус baseline-модели, сейчас
  `per_user_department_baseline_skeleton`;
- `baseline_window_days`: rolling window локальной baseline-истории;
- `user_baseline_available`: есть ли минимум samples для per-user сравнения;
- `department_baseline_available`: есть ли минимум samples для сравнения
  подразделений;
- `deviation_score`: score отклонения текущего дня от baseline;
- `baseline_samples`: количество накопленных user/department samples;
- `policy_version`: версия risk policy;
- `calculated_from`: список источников, участвовавших в расчете;
- `reasons`: DLP WARN/FAIL, open review queue, off-hours/weekend insights,
  просадки/аномалии Workforce, приложения без явного
  `application_weights` правила.

Baseline skeleton хранится локально в state каталоге портала как
`ueba-baseline-state.json`. Текущий день записывается атомарно по `report_date`
и не дублируется при повторном открытии отчета. Отклонение считается только
после накопления минимального количества исторических samples, поэтому первый
период эксплуатации честно показывает `*_baseline_available=false`.

Веса настраиваются через YAML policy:

- пример: `configs/detmir-ueba-risk-policy.example.yaml`;
- runtime-файл: `/etc/detmir-portal-ueba-policy.yaml`;
- env-путь: `DETMIR_PORTAL_UEBA_POLICY_PATH`.

Важно: текущий UEBA слой только ранжирует риск и объясняет причины. Он не
выполняет pfSense/NAC/SOAR actions и не меняет сетевые политики.

Для быстрой загрузки вкладки руководителя explainability-блок доступен отдельным
легким endpoint:

- `GET /api/workforce/policy/explain`

Полный `/api/reports` продолжает включать тот же `workforce_policy`, но вкладка
`Руководитель` не обязана грузить весь отчет ради одного KPI.

Минимальный JSON contract этого endpoint закреплен unit-тестом
`workforce_policy_explain_is_lightweight_payload`: портал не должен потерять
`formula`, `app_details[].matched_rule`, `app_details[].weight`,
`policy_audit`, `employee_details[].formula` и `employee_details[].reason`, а
легкий endpoint не должен случайно начать отдавать тяжелые поля полного отчета.

После изменения runtime-файла нужно перезапустить портал:

```bash
sudo systemctl restart detmir-portal.service
```

Если policy-файл отсутствует, портал показывает только нейтральный
`Индекс активности`. `Взвешенная активность` появляется только после настройки
role/application policy.

### Сравнение подразделений и тренды

Портал использует validated management snapshot из Worktime API и показывает:

- подразделения: coverage, активные пользователи, суммарное активное время;
- ответственных/владельцев процессов: coverage, активные пользователи,
  суммарное активное время;
- статус тренда по числу накопленных daily points.

Worktime API сохраняет daily history как агрегированные trend-points, без
полных строк сотрудников и без evidence. Runtime-настройки:

- `AW_WORKTIME_MANAGEMENT_HISTORY_DIR`;
- `AW_WORKTIME_MANAGEMENT_HISTORY_DAYS`;
- `AW_WORKTIME_MANAGEMENT_HISTORY_RETENTION_DAYS`.

Интерпретация трендов настраивается через customer policy:

- пример: `configs/worktime-interpretation-policy.example.json`;
- runtime-файл: `/etc/activitywatch/worktime-interpretation-policy.json`;
- env-путь: `AW_WORKTIME_MANAGER_INTERPRETATION_POLICY`.

Пример policy:

```json
{
  "overload_threshold": 0.92,
  "underload_threshold": 0.45,
  "drop_threshold_pct": 20,
  "night_work_after": "20:00",
  "weekend_work": true
}
```

`overload_threshold` и `underload_threshold` можно задавать дробью
`0.92`/`0.45` или процентом `92`/`45`; внутри они нормализуются к процентам.
Если policy-файл отсутствует или отдельное поле не задано, используются
env/default значения:

- `AW_WORKTIME_MANAGER_OVERLOAD_COVERAGE_PCT`;
- `AW_WORKTIME_MANAGER_TREND_MIN_POINTS`;
- `AW_WORKTIME_MANAGER_TREND_DELTA_PCT`;
- `AW_WORKTIME_MANAGER_OFF_HOURS_THRESHOLD_SECONDS`.

Слой автоматической интерпретации возвращает `trend_insights`:

- текущая недогрузка/перегрузка подразделения или ответственного;
- рост/падение portfolio activity несколько daily points подряд;
- стабильная недогрузка после накопления минимальной истории;
- резкая просадка подразделения/ответственного относительно своей нормы;
- активность вне рабочего окна;
- работа в выходной день.

Если истории мало, вывод должен быть честным: `history_insufficient`, без
продажи дневного среза как месячной аналитики.

Правило честной интерпретации:

- `daily_only` - есть только оперативный дневной срез, месячные выводы делать
  нельзя;
- `weekly_ready` - накоплено достаточно точек для недельного сравнения;
- `monthly_ready` - накоплено достаточно точек для месячного отчета владельцу.

Месячный отчет должен строиться только после накопления daily history. Если
история содержит один день, интерфейс обязан показывать дневной срез, а не
выдавать его за тренд месяца.

Корректная формулировка для продажи:

> DetMir помогает руководителю видеть загрузку сотрудников и бизнес-процессов
> без ручного просмотра логов и без подмены управленческой оценки простым
> учетом "сидел за компьютером".

## DetMir Security

Для ИБ, администратора и оператора расследований.

Показывает:

- DLP-сигналы: буфер обмена, печать, USB, файловые операции;
- severity/status технических сигналов;
- очередь DLP/case review;
- evidence metadata и доступные скриншоты;
- audit просмотра evidence и действий оператора.

В публичных и коммерческих материалах важно говорить аккуратно:

- `detections/cases` - это derived detections/cases;
- подтвержденным инцидентом событие становится после регламентной валидации;
- продукт не заявляется как сертифицированная DLP/SIEM/EDR/XDR/СЗИ.

## DetMir Forensics

Для разбора сложных событий и пост-инцидентной аналитики.

Показывает:

- цепочку событий ActivityWatch;
- Hayabusa/offline evidence workflow;
- связь технических сигналов, кейсов и артефактов;
- audit trail просмотра и обработки материалов;
- экспортируемые материалы для внутреннего расследования.

## Приоритет в демонстрации

Порядок показа владельцу бизнеса:

1. Workforce: индекс активности, загрузка, RDP/1С, рабочие приложения.
2. Commercial reports: ежедневный срез, KPI, Markdown/HTML отчет.
3. Security: DLP-сигналы и evidence.
4. Forensics: цепочка расследования, Hayabusa, кейсы.
5. Reliability: автономность, health-check, SLO, Grafana freshness.

Такой порядок снижает сопротивление вокруг темы "слежки" и переводит разговор
в плоскость эффективности, управляемости и доказуемой операционной картины.
