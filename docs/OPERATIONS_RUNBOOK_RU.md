# Operations Runbook

Документ описывает базовые эксплуатационные проверки AWatch-rus.

Ежедневный операторский цикл обслуживания, retention safety и безопасное
использование Pollinations AI для анализа sanitized evidence описаны отдельно:
[ежедневное обслуживание AWatch-rus](DAILY_MAINTENANCE_RU.md).

Автоматическое восстановление первичного AW API от подтвержденного
`poisoned datastore lock` описано отдельно:
[DetMir service reliability runbook](DETMIR_SERVICE_RELIABILITY_RUNBOOK_RU.md).

## Быстрая проверка

Проверить доступность:

```bash
curl -fsS http://<AWATCH_HOST>/healthz
curl -fsS http://<AWATCH_HOST>/readyz
curl -fsS http://<AWATCH_HOST>/metrics
```

Для защищенного reverse proxy использовать утвержденный URL и способ
аутентификации заказчика.

## healthz

`/healthz` используется для проверки, что процесс backend/portal отвечает.

Ожидается:

- HTTP 200 for healthy process;
- structured JSON or text according to current service implementation;
- no secrets in response.

## readyz

`/readyz` используется для проверки готовности обслуживать traffic.

Ожидается:

- ready status only when critical dependencies are acceptable;
- degraded status when reports/data sources are degraded;
- no false fully healthy status for degraded reports.

## metrics

`/metrics` используется для технического мониторинга.

Проверять:

- request counters;
- latency where exposed;
- degraded report counters where exposed;
- error counters.

## Smoke

Базовые smoke scripts:

```bash
node scripts/awatch-production-hardening-smoke.mjs
node scripts/detmir-pilot-demo-smoke.mjs
node scripts/deployment-readiness-smoke.mjs
```

Smoke должен выполняться:

- после установки;
- после обновления;
- перед demo;
- после recovery.

## Журналирование

Проверять:

```bash
journalctl -u <AWATCH_SERVICE> --since "30 minutes ago" --no-pager
systemctl status <AWATCH_SERVICE> --no-pager
systemctl --failed --no-pager
```

Не публиковать в issue/README:

- secrets;
- live hostnames;
- private IPs;
- customer evidence;
- user identifiers.

## Типовые сбои

### Portal недоступен

Проверить:

- process/service status;
- reverse proxy;
- firewall;
- TLS certificate;
- bind address;
- recent logs.

### Reports degraded

Проверить:

- data source freshness;
- worktime/report API timeout;
- stale cache usage;
- coverage;
- errors in logs.

### ActivityWatch API возвращает `503 poisoned lock`

Проверить, сработал ли primary recovery guard:

```bash
systemctl status detmir-aw-primary-recovery.timer --no-pager
journalctl -u detmir-aw-primary-recovery.service -n 80 --no-pager
sudo jq . /var/lib/detmir-aw-primary-recovery/latest.json
```

Если guard отключен, использовать manual sequence из
`DETMIR_SERVICE_RELIABILITY_RUNBOOK_RU.md`. Не удалять SQLite, lock или journal
файлы ActivityWatch вручную.

### Нет данных

Проверить:

- выбранный период;
- source availability;
- agent heartbeat;
- expected nodes;
- ingestion/storage status.

### Ролевая ошибка доступа

Проверить:

- selected role;
- `X-AWatch-Role` header where used;
- portal role gates;
- API endpoint scope.

## Диагностика

Минимальный набор:

```bash
curl -i http://<AWATCH_HOST>/healthz
curl -i http://<AWATCH_HOST>/readyz
curl -i http://<AWATCH_HOST>/api/reports?role=executive
curl -i http://<AWATCH_HOST>/api/actions?role=executive
```

Для production использовать TLS endpoint and approved auth.

## Эскалация

Эскалировать, если:

- `/healthz` недоступен;
- `/readyz` постоянно degraded;
- reports timeout повторяется;
- source freshness ниже пилотного SLA;
- backup restore не проходит;
- role gate отдает лишние данные.
