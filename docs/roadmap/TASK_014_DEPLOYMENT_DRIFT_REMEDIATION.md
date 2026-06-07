# docs/roadmap/TASK_014_DEPLOYMENT_DRIFT_REMEDIATION.md

Цель:

Устранить расхождение между:

* Demo Freeze v1;
* live DetMir runtime.

Проверить и внедрить в рабочий контур:

* /healthz
* /readyz
* /version
* /metrics
* request/correlation id
* Explainable KPI
* Risk Narrative
* Executive Action Center

Проверить:

* почему endpoints дают 404;
* почему browser conformance падает;
* почему Executive layer отсутствует;
* соответствует ли развернутый runtime текущему main;
* не используется ли устаревший build.

Результат:

Не добавлять новые функции.

Добиться того, чтобы:

live runtime == documented runtime

и

live runtime == Demo Freeze v1

````

Критерий успеха очень простой:

Сегодня:

```text
Browser smoke
FAIL

Production hardening smoke
FAIL
````

После задачи:

```text
Browser smoke
PASS

Production hardening smoke
PASS
```

на живом контуре.
