# AWatch-rus containment operator runbook

Дата: 2026-06-25.

Runbook для безопасной проверки containment-логики. Текущая реализация не
блокирует рабочие станции и не меняет сеть. Она только рассчитывает решение и
показывает, был бы quarantine рекомендован или отказан.

## 1. Сборка

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo build --manifest-path adk-rust/Cargo.toml -p containment-engine
```

## 2. Smoke в disabled/shadow режиме

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
bash scripts/containment_shadow_smoke.sh
```

Ожидаемо:

- JSON содержит `would_mutate=false`;
- `decision_status=disabled` для default example policy;
- нет изменений firewall, pfSense, AD, VLAN, routes.

## 3. Проверка shadow recommendation

Создайте временный policy с:

```json
{
  "enabled": true,
  "mode": "shadow"
}
```

на базе `configs/containment-policy.example.json`, затем выполните:

```bash
containment-engine decide \
  --policy /tmp/containment-policy-shadow.json \
  --finding configs/containment-finding.example.json \
  --pretty
```

Ожидаемо:

- `decision_status=shadow_recommended`;
- `would_mutate=false`;
- `rollback_plan_id` заполнен;
- `blockers=[]`.

## 4. Manual approval mode

`manual_approval` должен только поставить решение в состояние
`manual_approval_required`. Он не применяет block сам.

## 5. Auto mode

В текущей реализации `auto` может вернуть `auto_ready`, но `would_mutate=false`.
Это намеренно: decision layer сам не применяет блокировки.

Запрещено считать `auto_ready` фактической блокировкой. Это только решение
control plane.

## 6. Windows Firewall executor dry-run

Security Finding Inbox показывает подозрительные станции и фиксирует workflow
события. Портал не выполняет firewall apply. После `approved` и
`apply_requested` отдельный процесс `security-finding-inbox executor` может
выполнить контролируемый цикл `decide -> plan -> apply -> verify`, а при
ошибке `rollback`. По умолчанию executor работает dry-run/fail-closed.

Сгенерируйте план:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
containment-engine windows-firewall plan \
  --request configs/windows-firewall-containment-request.example.json \
  --pretty > /tmp/windows-firewall-plan.json
```

Проверьте `blockers`. Для корректного example они должны быть пустыми.

Dry-run apply:

```bash
containment-engine windows-firewall apply \
  --plan /tmp/windows-firewall-plan.json \
  --confirm-apply YES \
  --pretty
```

Ожидаемо:

- `execution_status=dry_run_commands_ready`;
- `would_mutate=false`;
- в JSON есть PowerShell-команды `New-NetFirewallRule`;
- реальные firewall-правила не создаются.

Verify dry-run:

```bash
containment-engine windows-firewall verify \
  --plan /tmp/windows-firewall-plan.json \
  --pretty
```

Rollback dry-run:

```bash
containment-engine windows-firewall rollback \
  --plan /tmp/windows-firewall-plan.json \
  --confirm-rollback YES \
  --pretty
```

## 7. Real Windows execution rules

Dry-run polling из центрального контура:

```bash
security-finding-inbox executor \
  --once \
  --dry-run \
  --containment-engine-bin /usr/local/bin/containment-engine \
  --policy /etc/activitywatch/containment-policy.json \
  --management-allowlist 10.10.10.10,10.10.10.11 \
  --blocked-remote-addresses 10.10.20.0/24,10.10.30.0/24
```

Реальный Windows Firewall apply допускается только на целевой Windows-станции:

```powershell
security-finding-inbox.exe executor `
  --once `
  --execute-local `
  --confirm-execute YES `
  --executor-host HOST-EXAMPLE `
  --containment-engine-bin C:\ProgramData\AWatch-rus\containment-engine.exe `
  --policy C:\ProgramData\AWatch-rus\containment-policy.json `
  --management-allowlist 10.10.10.10,10.10.10.11 `
  --blocked-remote-addresses 10.10.20.0/24,10.10.30.0/24
```

Executor откажется, если нет `approved` перед `apply_requested`, finding не
`suspected_infected`/`confirmed_infected`, management channel не проверен,
allowlist/block ranges пустые, host finding не совпадает с executor host для
local apply, containment policy возвращает blocker или Windows Firewall plan
содержит blockers.

`--execute-local` разрешён только для отдельного lab Windows host, где заранее
проверены:

- доступ с admin jump host;
- доступ к AWatch/Velociraptor management адресам;
- rollback command;
- out-of-band доступ, если firewall rule ошибочен;
- TTL и оператор, ответственный за возврат.

Не использовать широкие блокировки `Any`/`LocalSubnet`: Windows Firewall
block-правила могут перекрыть allow-правила и отрезать управление.

## 8. Когда можно расширять real containment executor

Только после выполнения условий:

- есть lab host;
- подтвержден management allowlist;
- есть rollback command;
- есть TTL rollback;
- есть audit log;
- `plan`, `apply`, `verify`, `rollback` покрыты тестами;
- auto-containment для серверов остается disabled.

## 9. Проверки перед commit

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
python3 scripts/public_secret_pattern_check.py
bash -n scripts/containment_shadow_smoke.sh
bash scripts/containment_shadow_smoke.sh
git diff --check

cd adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo fmt --all --check
cargo test -p containment-engine
cargo clippy -p containment-engine --all-targets -- -D warnings
```
