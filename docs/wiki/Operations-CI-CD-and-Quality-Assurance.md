# Operations, CI/CD, and Quality Assurance

## 8. Quality gates after `cc9e4a0`

Коммит усилил тестовое покрытие worktime path и сделал часть operational failures явными.

## Worktime tests

Новые/расширенные тесты:

```bash
python3 -m pytest aw-server/test_aw_worktime_api.py aw-server/test_aw_worktime_ui_bridge.py
```

Покрываются:

- in-process events cache;
- management report build locks;
- trend building с `precomputed_payloads`;
- foreground context fallback в UI bridge;
- active session id detection;
- нормализация watcher/window events.

Ожидаемый результат для текущего набора: `28 passed`.

## Autoheal changes

`aw-server/aw-worktime-autoheal.sh` изменен так, чтобы management warm-up не был обязательным probe для общей доступности worktime API.

Текущая логика:

- обязательные probes проверяют health/report path;
- если worktime API недоступен, autoheal перезапускает `aw-worktime-api.service`;
- management warm выполняется отдельно;
- failure management warm логируется, но не переводит весь health path в hard failure.

Timeout management warm увеличен:

```bash
WORKTIME_MANAGEMENT_WARM_TIMEOUT_SECONDS=60
```

Это снижает ложные перезапуски при тяжелой сборке management report.

## UI bridge service hardening

`aw-server/aw-worktime-ui-bridge.service` теперь использует:

```ini
StartLimitBurst=20
StartLimitIntervalSec=120
```

Большее значение `StartLimitBurst` нужно для recovery-сценариев после перезапуска AW server или временной недоступности buckets: bridge может несколько раз стартовать, пока API прогревается, не попадая сразу в systemd start-limit.

## Ansible validation

Перед rollout:

```bash
ansible-playbook --syntax-check ansible/deploy_aw_server.yml -i ansible/inventory.ini
ansible-playbook --syntax-check ansible/deploy_aw_windows.yml -i ansible/inventory.ini
```

Для Grafana exporters обязательно дополнительно проверить наличие:

```bash
AW_WORKTIME_INFLUX_TOKEN
AW_DLP_INFLUX_TOKEN
```

Playbook теперь не должен скрывать ошибку записи в Influx: если exporter не может писать в `aw_metrics`, deploy считается неуспешным.
