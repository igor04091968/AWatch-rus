# AWatch-rus containment policy

Дата: 2026-06-25.

Этот документ описывает безопасную политику автоматической/полуавтоматической
изоляции рабочих станций. Containment нужен для быстрого ограничения
распространения заражения, но не является автоматическим лечением,
remediation, EDR/XDR или сертифицированной СЗИ.

## Default posture

По умолчанию containment выключен:

```text
AW_CONTAINMENT_ENABLED=false
AW_CONTAINMENT_MODE=shadow
```

`shadow` означает: система рассчитывает рекомендацию и audit, но не меняет
firewall, pfSense, AD, VLAN, маршруты или состояние рабочих станций.

## Policy file

Default path:

```text
/etc/activitywatch/containment-policy.json
```

Repo example:

```text
configs/containment-policy.example.json
```

Критичные поля:

- `enabled`: глобальный opt-in;
- `mode`: `shadow`, `manual_approval`, `auto`;
- `default_ttl_minutes`: срок quarantine до rollback/review;
- `require_admin_channel_check`: запрещает блокировку, если управляемый канал
  не проверен;
- `allow_auto_for_servers`: по умолчанию `false`;
- `allowed_actions`: whitelist containment-действий;
- `management_allowlist`: каналы, которые должны оставаться доступными;
- `minimum_high_signals_for_auto`: минимальный порог high/critical signals.

## Safety rules

- Не включать `auto` до успешного shadow burn-in.
- Не включать auto-containment для серверов и domain controllers.
- Не запускать containment без rollback record.
- Не запускать containment, если будет потерян admin/management channel.
- Не применять широкие AD/OU/domain actions.
- Не удалять файлы, registry keys или процессы как часть containment.
- Не заявлять, что containment гарантированно остановил заражение.

## Decision threshold

Automatic quarantine допускается только если:

- host role is `workstation`;
- host не входит в critical infrastructure denylist;
- есть один `critical` signal или несколько `high` signals;
- management-channel precheck passed;
- action есть в `allowed_actions`;
- rollback record создан успешно.

## Security Finding Inbox handoff

Security Finding Inbox (`docs/SECURITY_FINDING_INBOX_RU.md`) является входной
очередью для подозрительных рабочих станций. Он хранит findings и workflow
events в ClickHouse, показывает их в DetMir Portal. Портал не выполняет
containment самостоятельно.

Workflow `apply_requested` означает только операторский запрос на применение.
Фактическое применение идет через отдельный процесс
`security-finding-inbox executor`, который повторно проверяет `approved`,
запускает `containment-engine decide`, строит Windows Firewall plan, затем
выполняет `apply`, `verify` и при ошибке `rollback`. Реальная мутация firewall
разрешена только на целевой Windows-станции при `--execute-local`,
`--confirm-execute YES` и совпадении `--executor-host` с finding host.

## Current implementation status

Реализован первый безопасный слой:

- Rust CLI `containment-engine`;
- strict JSON policy/finding parsing;
- `disabled`, `shadow`, `manual_approval`, `auto` decision states;
- server/unknown host roles refused for auto mode by default;
- `would_mutate=false` for current implementation;
- separate Windows Firewall executor interface:
  `plan`, `apply`, `verify`, `rollback`;
- Windows Firewall executor defaults to dry-run command generation unless
  `--execute-local` and explicit confirmation are used.

pfSense/AD/VLAN mutation paths are not implemented.

## Windows Firewall executor

Executor input example:

```text
configs/windows-firewall-containment-request.example.json
```

The executor is deliberately separate from decision making:

```bash
containment-engine windows-firewall plan \
  --request configs/windows-firewall-containment-request.example.json \
  --pretty > /tmp/windows-firewall-plan.json

containment-engine windows-firewall apply \
  --plan /tmp/windows-firewall-plan.json \
  --confirm-apply YES \
  --pretty

containment-engine windows-firewall verify \
  --plan /tmp/windows-firewall-plan.json \
  --pretty

containment-engine windows-firewall rollback \
  --plan /tmp/windows-firewall-plan.json \
  --confirm-rollback YES \
  --pretty
```

Without `--execute-local`, `apply` and `rollback` return generated PowerShell
commands and `would_mutate=false`.

With `--execute-local`, execution is allowed only on a Windows host and only
after explicit confirmation. On non-Windows hosts the executor fails closed.

## Windows Firewall guardrails

- `management_allowlist` is mandatory.
- `blocked_remote_addresses` must be explicit IPs/subnets.
- Broad block targets such as `Any`, `*`, `LocalSubnet`, `Internet`,
  `Intranet` are refused.
- The executor does not change Windows Firewall profile defaults.
- The executor does not disable interfaces, routes, users, services or
  processes.
- Every plan includes rollback through `Remove-NetFirewallRule -Group ...`.
- A successful dry-run is not evidence that the workstation has been isolated.
