# Security Finding Inbox

Дата: 2026-06-25.

Security Finding Inbox - это очередь подозрительных рабочих станций для
связки Hayabusa/Sigma, Velociraptor, AWatch context и ручных ИБ-сигналов.
Очередь нужна для контролируемого процесса:

```text
finding -> triage -> decide -> plan -> approve -> apply_requested -> executor -> verify/rollback
```

Важно: портал inbox сам не применяет Windows Firewall, pfSense, AD или VLAN
изменения. Он фиксирует findings и workflow-события. Реальное применение
делается отдельным процессом `security-finding-inbox executor`, который
вызывает `containment-engine windows-firewall plan/apply/verify/rollback`.
По умолчанию executor работает безопасно: dry-run/fail-closed, без локального
изменения firewall.

## Компоненты

- ClickHouse schema:
  `clickhouse-1c/security/security_finding_inbox.sql`
- normalized finding example:
  `configs/security/security-finding.example.json`
- ingest/workflow CLI:
  `adk-rust/crates/security-finding-inbox`
- executor CLI:
  `security-finding-inbox executor`
- portal API:
  `/api/security/findings`
  `/api/security/findings/workflow`
- portal page:
  `Подозрительные станции`

## ClickHouse tables

`security_findings`

- normalized finding records;
- source: `hayabusa`, `sigma`, `velociraptor`, `awatch`, `manual`, `dlp`;
- states: `new`, `suspected_infected`, `confirmed_infected`, `contained`,
  `released`, `false_positive`;
- recommended action remains a recommendation, not a mutation.

`security_finding_workflow_events`

- append-only workflow audit;
- event types: `decide_requested`, `plan_requested`, `approved`,
  `apply_requested`, `verify_requested`, `rollback_requested`, `rejected`,
  `false_positive`, plus executor audit events:
  `executor_plan_ready`, `executor_apply_succeeded`,
  `executor_apply_failed`, `executor_verify_succeeded`,
  `executor_verify_failed`, `executor_refused`,
  `executor_rollback_succeeded`, `executor_rollback_failed`;
- portal writes only workflow events.

`security_finding_inbox`

- latest-state view for portal/dashboard;
- filters released/rejected/false-positive rows out of the active queue.

## Ingest

Build:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo build --manifest-path adk-rust/Cargo.toml -p security-finding-inbox
```

Validate sample:

```bash
security-finding-inbox validate \
  --input configs/security/security-finding.example.json
```

Dry-run ingest:

```bash
security-finding-inbox ingest \
  --input configs/security/security-finding.example.json \
  --dry-run
```

Apply schema and ingest into ClickHouse:

```bash
security-finding-inbox ingest \
  --input configs/security/security-finding.example.json \
  --clickhouse-url http://10.10.10.2:8123 \
  --database analytics_1c \
  --user "$CLICKHOUSE_USER" \
  --password "$CLICKHOUSE_PASSWORD" \
  --apply-schema
```

The CLI accepts a single JSON object, JSON array, or JSONL.

### Real Hayabusa source

После `aw-hayabusa process-inbox` реальный источник находится в
`/opt/hayabusa/state/latest-intake.json`. CLI читает `report_dir`, анализирует
`timeline.jsonl`, logon summaries и строит normalized finding:

```bash
security-finding-inbox ingest-hayabusa \
  --intake /opt/hayabusa/state/latest-intake.json \
  --min-severity medium \
  --clickhouse-url http://127.0.0.1:8123 \
  --database analytics_1c
```

Для автоматического подключения Hayabusa drop/autoprocess:

```bash
AW_SECURITY_FINDING_INBOX_ENABLED=true
AW_SECURITY_FINDING_INBOX_BIN=/usr/local/bin/security-finding-inbox
AW_SECURITY_FINDING_INBOX_MIN_SEVERITY=medium
```

`AW_SECURITY_FINDING_INBOX_REQUIRED=false` оставляет forensic pipeline живым,
если ClickHouse или inbox CLI временно недоступны. В `true`-режиме ошибка
ingest считается operational failure.

### Real Velociraptor source

Velociraptor artifact JSON/JSONL можно загрузить через generic adapter:

```bash
security-finding-inbox ingest-velociraptor-json \
  --input /path/to/velociraptor-artifact.jsonl \
  --default-severity high \
  --clickhouse-url http://127.0.0.1:8123 \
  --database analytics_1c
```

Adapter ищет стандартные поля `Hostname`, `Artifact`, `Severity`, `Message`,
`User`, `IP`. Если формат артефакта отличается, используйте normalized
`security-finding-inbox ingest --input ...`.

## Portal workflow

Open:

```text
DetMir Portal -> Подозрительные станции
```

The page shows:

- host/user/IP/department;
- severity/confidence/score;
- source/rule;
- state and latest workflow status;
- recommended action;
- workflow buttons.

Portal buttons record only workflow events:

- `decide`: request decision calculation;
- `plan`: request containment plan;
- `approve`: operator approval record;
- `apply`: request to perform apply outside the portal;
- `rollback`: rollback request record.

The portal does not run `containment-engine`, PowerShell, firewall commands or
network changes.

## Executor handoff

Executor читает из ClickHouse только те findings, где:

- последний workflow event: `apply_requested`;
- status: `apply_pending`;
- ранее есть `approved`;
- еще нет `executor_apply_succeeded`, `executor_apply_failed`,
  `executor_refused` или rollback terminal event.

Dry-run executor:

```bash
security-finding-inbox executor \
  --once \
  --dry-run \
  --containment-engine-bin /usr/local/bin/containment-engine \
  --policy /etc/activitywatch/containment-policy.json \
  --management-allowlist 10.10.10.10,10.10.10.11 \
  --blocked-remote-addresses 10.10.20.0/24,10.10.30.0/24
```

Linux systemd example for central dry-run/polling mode:

```text
ops/systemd/aw-security-finding-executor.service
```

Real local Windows apply is allowed only when all conditions are true:

- executor runs on the target Windows workstation;
- `--execute-local` is set;
- `--confirm-execute YES` is set;
- `--executor-host` or local `COMPUTERNAME` matches finding `host`;
- containment policy returns `manual_approval_required` or `auto_ready`;
- management allowlist and blocked remote ranges are explicit;
- generated Windows Firewall plan has no blockers.

Example on the target Windows host:

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

Executor writes `executor_*` workflow events back into ClickHouse. It does not
update or delete source findings.

## Manual containment handoff

After a finding is approved:

1. Build or review a containment policy/finding.
2. Run:

```bash
containment-engine decide \
  --policy /etc/activitywatch/containment-policy.json \
  --finding /path/to/finding.json \
  --pretty
```

3. Build Windows Firewall request with explicit management allowlist.
4. Run:

```bash
containment-engine windows-firewall plan \
  --request /path/to/windows-firewall-request.json \
  --pretty > /tmp/fw-plan.json
```

5. Confirm `blockers=[]`.
6. Dry-run:

```bash
containment-engine windows-firewall apply \
  --plan /tmp/fw-plan.json \
  --confirm-apply YES \
  --pretty
```

7. Real apply only on the target Windows host:

```powershell
containment-engine.exe windows-firewall apply `
  --plan C:\Temp\fw-plan.json `
  --confirm-apply YES `
  --execute-local `
  --pretty
```

## Guardrails

- Do not place raw employee logs, secrets, passwords or customer identifiers in
  findings.
- Do not treat `apply_requested` as successful containment.
- Do not run broad `Any`/`LocalSubnet` firewall blocks.
- Do not enable automatic action for servers/domain controllers.
- Keep GitHub/portal evidence separate from Russian registry release evidence.
- Keep DLP optional: Hayabusa/Velociraptor findings can continue while heavy DLP
  runtime is disabled.
