# Low-cost Sigma/Hayabusa/Velociraptor containment addon

Дата: 2026-06-25.

Цель: добавить в AWatch-rus дешевый, воспроизводимый и отключаемый слой
security containment + forensics для организаций без зрелого SIEM/EDR. Главный
смысл модуля - быстро ограничить дальнейшее распространение заражения или
подозрительной активности с рабочей станции, сохранив управляемый канал
расследования и восстановления.

Это дополнение не делает AWatch-rus сертифицированной DLP/SIEM/EDR/XDR/СЗИ и
не заменяет штатные средства защиты. Автоматическая блокировка здесь означает
policy-approved containment/quarantine, а не автоматическое лечение системы.

## Upstream basis

- Hayabusa: fast Windows event log forensics timeline generator and threat
  hunting tool, written in Rust, using Sigma-compatible Hayabusa rules.
- Hayabusa supports single-host/live analysis, offline analysis of collected
  logs, and enterprise-wide use through a Velociraptor artifact.
- Hayabusa outputs timeline/results suitable for CSV, JSON/JSONL and HTML
  reports.
- `Windows.Hayabusa.Monitoring` in Velociraptor Curated Sigma is an artifact
  intended to triage a Windows host and is based on `Windows.Sigma.BaseEvents`.
- Velociraptor is an endpoint visibility and collection tool using VQL
  artifacts. Its normal deployment is server + clients, but it also supports
  offline collectors and command-line artifact execution.

Primary references:

- https://github.com/Yamato-Security/hayabusa
- https://github.com/Yamato-Security/hayabusa/wiki/About-Hayabusa
- https://github.com/Yamato-Security/hayabusa-rules
- https://sigma.velocidex.com/docs/artifacts/windows.hayabusa.monitoring/
- https://github.com/Velocidex/velociraptor
- https://docs.velociraptor.app/docs/deployment/

## Product positioning

Рабочее название модуля:

```text
AWatch-rus Low-Cost Containment Pack
```

Назначение:

- быстро получить полезный containment + DFIR/threat-hunting слой там, где
  нет SIEM/EDR;
- автоматически или полуавтоматически изолировать подозрительно зараженную
  рабочую станцию от критичных сегментов;
- сохранить минимальный управляемый канал: AWatch-rus/Velociraptor server,
  администраторский jump/VPN, DNS/NTP при необходимости;
- запускать Hayabusa/Sigma-анализ EVTX и Velociraptor artifact collection;
- давать владельцу и администратору понятные findings, timeline и evidence;
- связывать findings с AWatch-rus cases и operator/forensics views;
- оставаться optional и выключаемым без деградации Workforce core.

Запрещенные claims:

- не писать, что это SIEM replacement;
- не писать, что это DLP replacement;
- не писать, что это EDR/XDR;
- не писать, что это сертифицированная СЗИ;
- не писать, что автоматическое remediation включено;
- не писать, что automatic containment гарантированно остановит заражение;
- не писать, что threat detection ML/LLM-based;
- не писать, что найденные события являются доказанной атакой без ручной
  проверки.

Допустимая формулировка:

```text
Optional low-cost containment, security analytics and forensics layer based on
open-source Hayabusa/Sigma/Velociraptor workflows.
```

## Containment objective

Модуль должен отвечать на вопрос:

```text
Как максимально быстро ограничить рабочую станцию, которая выглядит зараженной,
чтобы она не заражала соседние машины и не продолжала утечку/распространение?
```

Необходимо разделять:

- `suspected_infected` - есть правила/сигналы/аномалии, достаточные для
  карантина по политике организации;
- `confirmed_infected` - есть ручное подтверждение администратора/ИБ;
- `contained` - станция технически ограничена;
- `released` - карантин снят вручную или по документированному rollback.

Containment actions должны быть обратимыми, журналируемыми и ограниченными по
blast radius. По умолчанию допускается `shadow` или `manual_approval`; fully
automatic quarantine включается только отдельным флагом и только после
allowlist/rollback проверки.

## Architecture

### Modes

1. `disabled`
   - default для conservative deployment;
   - все readiness checks возвращают disabled-state;
   - Workforce/ActivityWatch core не зависит от модуля.

2. `hayabusa_offline`
   - текущий базовый режим;
   - Windows scheduled task экспортирует EVTX zip;
   - серверный `aw-hayabusa-drop.path` принимает package;
   - `aw-hayabusa-autoprocess` валидирует zip, process-inbox, quarantine.

3. `velociraptor_offline_collector`
   - для бедных/малых контуров без постоянно работающего Velociraptor server;
   - AWatch-rus собирает/хранит signed offline collector bundle;
   - запуск collector выполняется вручную или scheduled task;
   - результаты импортируются как artifact bundle.

4. `velociraptor_server_clients`
   - optional managed mode;
   - Linux server рядом с AW/Proxmox или отдельной VM;
   - Windows clients ставятся только явным Ansible-флагом;
   - используется для управляемого запуска `Windows.Hayabusa.Monitoring`.

5. `containment_shadow`
   - decision engine считает, что сделал бы, но ничего не блокирует;
   - безопасный default для пилота;
   - используется для настройки правил и false-positive анализа.

6. `containment_manual_approval`
   - система формирует containment recommendation;
   - администратор подтверждает действие в CLI/портале;
   - все действия пишутся в audit trail.

7. `containment_auto`
   - система сама применяет заранее разрешенные quarantine-действия;
   - включается только явным флагом;
   - требует allowlist, rollback TTL и проверку сохранения admin channel.

### Boundaries

Core remains:

- ActivityWatch server;
- Workforce reports;
- RDP/window/AFK/worktime collectors;
- 1C/ClickHouse analytics;
- portal health/readiness;
- Hayabusa drop quarantine hardening.

Optional containment/forensics layer:

- Hayabusa binary and rules;
- Sigma/Hayabusa curated rules cache;
- Velociraptor binary/config/artifacts;
- Velociraptor clients/offline collectors;
- artifact result import;
- findings summary and case links;
- containment decision engine;
- containment executor for approved channels.

Containment channels:

- Windows host firewall quarantine:
  allow only AWatch-rus/Velociraptor server, DNS/NTP if required, and admin
  jump/VPN;
- pfSense/network gateway block:
  block workstation IP/MAC from lateral/internal segments, keep management
  exception;
- switch/VLAN quarantine when supported:
  move port/client to quarantine VLAN through explicit integration;
- Windows local containment:
  stop risky shares/services, disable outbound SMB/RDP to peers, collect
  evidence;
- Active Directory actions, if configured:
  disable only the workstation account or user session by policy, never broad
  OU/domain actions by default.

Non-goals:

- deleting malware;
- cleaning registry/files;
- killing arbitrary processes based on weak signal;
- disabling domain-wide accounts;
- blocking servers/shared infrastructure automatically;
- hiding the host from administrators.

No hot-path dependency:

- portal first screen must not wait for Velociraptor;
- Workforce reports must not query Velociraptor;
- readiness must not fail when module is disabled;
- heavy artifact execution must be timer/manual/background only.

## Proposed configuration

Ansible group vars:

```yaml
aw_forensics_pack_enabled: false
aw_hayabusa_enabled: true
aw_hayabusa_rules_enabled: true
aw_hayabusa_rules_version: "pinned"
aw_hayabusa_rules_update_enabled: false

aw_velociraptor_enabled: false
aw_velociraptor_mode: "disabled" # disabled|offline_collector|server_clients
aw_velociraptor_version: "pinned"
aw_velociraptor_server_bind_host: "127.0.0.1"
aw_velociraptor_public_enabled: false
aw_velociraptor_artifact_pack_enabled: true
aw_velociraptor_hayabusa_artifact_enabled: true

aw_forensics_store_raw_artifacts: false
aw_forensics_raw_retention_days: 7
aw_forensics_result_retention_days: 90
aw_forensics_max_parallel_jobs: 1
aw_forensics_max_job_minutes: 30
aw_forensics_cpu_quota_pct: 25
aw_forensics_io_nice: true

aw_containment_enabled: false
aw_containment_mode: "shadow" # shadow|manual_approval|auto
aw_containment_default_ttl_minutes: 60
aw_containment_require_admin_channel_check: true
aw_containment_allow_auto_for_servers: false
aw_containment_allowed_actions:
  - windows_firewall_quarantine
  - pfsense_host_block
aw_containment_management_allowlist:
  - "aw_server"
  - "velociraptor_server"
  - "admin_jump_host"
```

Runtime env:

```text
AW_FORENSICS_PACK_ENABLED=false
AW_HAYABUSA_ENABLED=true
AW_VELOCIRAPTOR_ENABLED=false
AW_VELOCIRAPTOR_MODE=disabled
AW_FORENSICS_STORE_RAW_ARTIFACTS=false
AW_CONTAINMENT_ENABLED=false
AW_CONTAINMENT_MODE=shadow
```

## Data flow

### Existing Hayabusa path

```text
Windows EVTX export
  -> zip + sidecars
  -> /opt/activitywatch/aw-rus-ops/drop
  -> aw-hayabusa-autoprocess
  -> validate package
  -> accept/process-inbox
  -> result_dir/latest-intake.json
  -> case link / portal summary
  -> quarantine on bad package
```

### New Velociraptor path

```text
Velociraptor artifact run
  -> Windows.Hayabusa.Monitoring / custom artifact
  -> Velociraptor result export
  -> AWatch-rus import directory
  -> schema validation
  -> derived findings JSON/SQLite
  -> optional case link
  -> portal forensics summary
```

### Containment path

```text
Finding/signals
  -> confidence and policy evaluation
  -> containment decision record
  -> admin-channel precheck
  -> shadow/manual/auto execution
  -> verify containment
  -> case/audit record
  -> TTL/rollback queue
```

Raw artifacts and derived results must be separated:

- raw EVTX/result bundles: restricted evidence storage;
- derived findings: sanitized AWatch-rus views;
- operator notes/case links: case database;
- public/demo exports: no raw hostnames, users, IPs, paths or secrets.

## Security and privacy guardrails

- Store no secrets in repo, docs, demo data or screenshots.
- Do not commit generated Velociraptor config with private keys/client secrets.
- Do not expose Velociraptor GUI publicly by default.
- Default server bind should be loopback or private VPN-only address.
- Require explicit operator action for client deployment.
- Require retention policy for raw artifacts.
- Require redaction for export/demo packs.
- Require audit log for artifact imports, deletes and case links.
- Treat Velociraptor outputs as untrusted input: validate schema, size, paths
  and timestamps before import.
- Never execute arbitrary downloaded artifacts without pinning/checksums.
- Never run containment if management channel would be lost.
- Never auto-contain servers unless explicitly allowed and tested.
- Always create rollback record before applying a block.
- Always include TTL or manual release path.
- Always log who/what triggered containment, which signals were used and which
  network paths remain allowed.

## Containment decision model

Inputs:

- high/critical Hayabusa/Sigma rule hits;
- suspicious Windows event sequence from Velociraptor artifact;
- AWatch-rus endpoint signals such as mass file changes, unusual process/file
  behavior, DLP/security signal spikes;
- administrator manual flag.

Decision fields:

```json
{
  "host": "HOST",
  "host_role": "workstation",
  "state": "suspected_infected",
  "confidence": "medium|high|critical",
  "signals": ["hayabusa:rule-id", "velociraptor:artifact"],
  "recommended_action": "windows_firewall_quarantine",
  "mode": "shadow|manual_approval|auto",
  "ttl_minutes": 60,
  "management_channel_checked": true,
  "rollback_plan_id": "opaque-id"
}
```

Minimum threshold for automatic quarantine:

- host role is workstation;
- host is not in denylist of critical infrastructure;
- at least one critical signal or multiple high-confidence signals;
- management channel precheck passed;
- containment action is in allowlist;
- rollback record successfully written.

## Current implementation status

Implemented first safe layer:

- Rust CLI `containment-engine`;
- strict JSON parsing for policy/finding input;
- example files:
  `configs/containment-policy.example.json`,
  `configs/containment-finding.example.json`,
  `configs/windows-firewall-containment-request.example.json`;
- disabled-by-default Ansible/env configuration;
- `shadow`, `manual_approval` and `auto` decision states;
- automatic containment refused for non-workstation roles by default;
- `would_mutate=false` in current implementation;
- Windows Firewall executor interface:
  `plan`, `apply`, `verify`, `rollback`;
- Windows Firewall dry-run generates PowerShell `New-NetFirewallRule`,
  `Get-NetFirewallRule` and `Remove-NetFirewallRule` commands;
- Windows Firewall execution is fail-closed without explicit confirmation and
  `--execute-local`;
- Security Finding Inbox:
  ClickHouse schema, Rust ingest CLI, Hayabusa/Velociraptor source adapters,
  portal page `Подозрительные станции` and separate executor process for
  approved `apply_requested` workflow;
- smoke script:
  `bash scripts/containment_shadow_smoke.sh`;
- operator/policy docs:
  `docs/CONTAINMENT_OPERATOR_RUNBOOK_RU.md`,
  `docs/CONTAINMENT_POLICY_RU.md`.

Not implemented yet:

- production-verified Windows Firewall mutation on lab/real workstations;
- real pfSense alias/table mutation;
- AD/VLAN executor;
- TTL rollback service;
- portal containment action execution. The current portal records workflow
  events only and does not mutate firewall/network state; mutation is reserved
  for `security-finding-inbox executor` with explicit local Windows
  confirmation.

## Codex implementation plan

### Phase 0. Architecture and docs only

Files:

- `docs/LOW_COST_SIGMA_HAYABUSA_VELOCIRAPTOR_ADDON_RU.md`;
- `docs/PROJECT_STATUS_RU.md`;
- `docs/REGISTRY_FUNCTIONAL_SCOPE_RU.md`;
- `README.md`.

Tasks:

- record addon scope and non-goals;
- document upstream references and license/supply-chain review requirement;
- state that module is planned/optional until implemented;
- keep forbidden SIEM/DLP/СЗИ claims blocked.

Acceptance:

- docs mention optional low-cost containment/forensics layer;
- no runtime/API/UI/product code change;
- secret scan and diff check pass.

### Phase 1. Inventory current Hayabusa implementation

Files:

- `aw-server/hayabusa/README.md`;
- `aw-server/hayabusa/aw-hayabusa.sh`;
- `adk-rust/crates/hayabusa-tools/`;
- `windows/export-evtx-for-hayabusa.ps1`;
- `windows/export-upload-hayabusa-to-aw-server.ps1`;
- `ansible/deploy_aw_server.yml`;
- `ansible/deploy_aw_windows.yml`.

Tasks:

- document installed binaries, units, timers, directories and retention;
- verify current drop/inbox/quarantine behavior;
- add a manifest file for current Hayabusa server bundle;
- add a read-only status command if missing.

Acceptance:

- `aw-hayabusa doctor` remains green;
- bad zip quarantine behavior remains intact;
- no change to DLP disabled runtime state.

### Phase 2. Supply-chain manifest and pinned downloads

New files:

- `third_party/forensics/manifest.json`;
- `scripts/prepare_forensics_binaries.sh`;
- `docs/FORENSICS_SUPPLY_CHAIN_RU.md`.

Tasks:

- define pinned versions for Hayabusa, Hayabusa rules and Velociraptor;
- define SHA256 checksums and source URLs;
- support offline cache directory;
- fail closed if checksum mismatch;
- never auto-update rules in production unless explicitly enabled.

Acceptance:

- dry-run prints planned downloads only;
- checksum verification works on cached fixture;
- no network required for deploy when cache exists.

### Phase 3. Optional Velociraptor server install

New/changed files:

- `ansible/group_vars/all.yml`;
- `ansible/group_vars/all.example.yml`;
- `ansible/deploy_aw_server.yml`;
- `ops/systemd/velociraptor.service`;
- `docs/VELOCIRAPTOR_DEPLOYMENT_RU.md`.

Tasks:

- add `aw_velociraptor_enabled=false` default;
- install Velociraptor binary only when enabled;
- generate config only on target host, not in repo;
- bind to loopback/private address by default;
- store datastore under `/var/lib/velociraptor`;
- store config under `/etc/velociraptor`;
- add systemd service with resource limits;
- avoid public exposure unless explicitly configured.

Acceptance:

- disabled mode creates no running service;
- enabled mode installs service and returns local health;
- generated config is not committed;
- Ansible syntax check passes.

### Phase 4. Velociraptor client/offline collector packaging

Files:

- `ansible/deploy_aw_windows.yml`;
- `windows/ActivityWatch.Windows.Common.psm1`;
- `windows/validate-deployment.ps1`;
- optional `windows/install-velociraptor-client.ps1`.

Tasks:

- add explicit deployment mode:
  `disabled|offline_collector|client_service`;
- package client installer/offline collector from pinned binary/config;
- install client service only when explicitly enabled;
- keep scheduled/manual offline collector for low-cost mode;
- log to `C:\ProgramData\AWatch-rus\logs\velociraptor-*.log`;
- include service/task checks in validation only when enabled.

Acceptance:

- disabled mode leaves Windows host untouched;
- offline collector can run and produce an export bundle;
- service mode reports healthy enrollment without exposing credentials.

### Phase 5. Hayabusa/Sigma artifact integration

Files:

- `third_party/forensics/artifacts/`;
- `scripts/import_velociraptor_artifact_pack.sh`;
- `docs/HAYABUSA_SIGMA_RULES_RU.md`.

Tasks:

- import/prepare `Windows.Hayabusa.Monitoring` artifact pack;
- document mapping to Hayabusa rules;
- create curated profile:
  `low-cost-default`, `incident`, `full`;
- add noisy-rule tuning file;
- require version metadata in every run.

Acceptance:

- artifact pack import is reproducible;
- rules profile can be listed without running collection;
- config supports small-host low-resource default.

### Phase 6. AWatch-rus result import

Prefer Rust.

New crate or extension:

- `adk-rust/crates/forensics-importer`;
  or extend `adk-rust/crates/hayabusa-tools`.

Tasks:

- import Hayabusa JSON/JSONL/CSV summary;
- import Velociraptor artifact result export;
- normalize to derived finding schema:
  `source`, `host`, `time`, `rule`, `level`, `mitre`, `summary`,
  `evidence_ref`, `case_id`, `tool_version`, `rules_version`;
- reject oversized, malformed and path-traversal payloads;
- write derived SQLite/JSON under `/var/lib/activitywatch/forensics`;
- do not copy raw artifacts unless `AW_FORENSICS_STORE_RAW_ARTIFACTS=true`.

Acceptance:

- unit tests cover malformed JSON, oversized file, path traversal, empty result;
- fixture import produces stable output;
- raw-sensitive data is not rendered in default portal view.

### Phase 7. Containment control plane

Prefer Rust.

New crate or extension:

- `adk-rust/crates/containment-engine`;
  or extend `adk-rust/crates/forensics-importer` with a separate module.

Tasks:

- define containment decision schema and audit log;
- add policy file:
  `/etc/activitywatch/containment-policy.json`;
- add host role model:
  `workstation|server|domain_controller|unknown`;
- add safe defaults:
  `enabled=false`, `mode=shadow`, server auto-containment disabled;
- implement decision evaluation from imported findings;
- implement dry-run/shadow output;
- implement manual approval queue;
- implement rollback record format.

Acceptance:

- unit tests cover workstation/server/unknown host roles;
- automatic action is refused for server/unknown role by default;
- no action runs if management channel precheck fails;
- shadow mode produces audit record and does not mutate host/network.

### Phase 8. Containment executors

Executor targets:

- Windows firewall quarantine through PowerShell/Rust Windows helper;
- pfSense alias/table block through explicit API/SSH integration;
- optional switch/VLAN integration only behind feature flag.

Tasks:

- implement executor interface:
  `plan`, `apply`, `verify`, `rollback`;
- first implemented executor: Windows Firewall explicit management allowlist
  plus explicit block ranges, without broad `Any`/`LocalSubnet` block and
  without default firewall profile changes;
- apply pfSense host block using IP/MAC only after current lease/identity
  verification;
- store rollback before mutation;
- add TTL-based rollback timer;
- add emergency release command.

Acceptance:

- fixture mode shows exact firewall/pfSense plan;
- apply refuses empty allowlist;
- verify confirms blocked lateral path and allowed management path;
- rollback restores previous rules;
- logs contain no secrets.

### Phase 9. Portal and API integration

Files:

- `adk-rust/crates/detmir-portal/`;
- `docs/PORTAL_API_CONTRACTS_RU.md`;
- `docs/DETMIR_CURRENT_STATE_RU.md`.

Tasks:

- add optional forensics module state:
  `disabled|not_configured|ready|degraded`;
- show derived findings count, latest run, severity histogram;
- show containment state:
  `disabled|shadow|recommended|contained|rollback_pending|released`;
- show clear action buttons only for authorized admin/security roles;
- link to case/evidence only by opaque ID;
- do not block Workforce first screen;
- do not include raw artifacts in frontend payload.

Acceptance:

- with module disabled, portal shows disabled-state and remains fast;
- with fixture findings, portal renders summary;
- with fixture containment recommendation, portal renders action state without
  applying action;
- Playwright smoke confirms no endless loading and no raw sensitive fields.

### Phase 10. Health/readiness/checks

Files:

- `adk-rust/crates/detmir-check/`;
- `adk-rust/crates/detmir-readiness/`;
- `scripts/detmir-full-diagnostics/aw-contour-diag.sh`;
- `scripts/aw-contour-diag.sh`;
- `check-aw-full.sh`.

Tasks:

- add optional forensics status checks;
- disabled mode must be OK/Skipped, not fail;
- add optional containment status checks;
- enabled mode checks:
  Velociraptor service, artifact pack, latest run age, importer health,
  queue/quarantine counts, containment executor health;
- add resource pressure checks for long-running scans.

Acceptance:

- disabled mode produces `forensics:mode=disabled`;
- disabled containment mode produces `containment:mode=disabled`;
- enabled mode fails closed on stale/broken artifact importer;
- containment auto mode fails closed if rollback store or admin-channel precheck
  is unavailable;
- checks do not restart services unless explicit autoheal mode exists.

### Phase 11. Runtime safety and resource budgets

Files:

- systemd units/timers;
- Ansible vars;
- docs runbooks.

Tasks:

- enforce `Nice`, `IOSchedulingClass`, CPU quota and timeout for heavy scans;
- serialize jobs through lock file;
- add cancellation/timeout behavior;
- quarantine failed artifact runs;
- keep `aw-server-rust`, worktime API, ClickHouse and portal out of the scan
  critical path.
- enforce containment mutation lock so two block/unblock actions cannot race.

Acceptance:

- two concurrent scan requests do not run two heavy jobs;
- timeout leaves a clear failed run record;
- core health remains green under disabled mode.
- rollback timer is tested and idempotent.

### Phase 12. Documentation and operator runbooks

New docs:

- `docs/VELOCIRAPTOR_DEPLOYMENT_RU.md`;
- `docs/FORENSICS_SUPPLY_CHAIN_RU.md`;
- `docs/FORENSICS_OPERATOR_RUNBOOK_RU.md`;
- `docs/FORENSICS_RETENTION_POLICY_RU.md`;
- `docs/FORENSICS_PRIVACY_GUARDRAILS_RU.md`.
- `docs/CONTAINMENT_OPERATOR_RUNBOOK_RU.md`;
- `docs/CONTAINMENT_POLICY_RU.md`.

Tasks:

- describe installation modes;
- describe offline collector workflow;
- describe artifact run, import, case link and cleanup;
- describe forbidden data in screenshots/demo packs;
- document rollback and disable commands.
- describe quarantine policy, management allowlist, manual approval, emergency
  release and TTL rollback.

Acceptance:

- admin can install disabled/offline/server modes from docs;
- admin can test shadow containment safely before auto mode;
- docs clearly say GitHub/public demo is not evidence storage;
- no claim of SIEM/DLP/EDR/СЗИ replacement.

### Phase 13. Tests and gates

Required checks:

```bash
python3 scripts/public_secret_pattern_check.py
bash -n scripts/prepare_forensics_binaries.sh
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml --syntax-check
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_windows.yml --syntax-check

cd adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo fmt --all --check
cargo test -p hayabusa-tools
cargo test -p forensics-importer
cargo test -p containment-engine
cargo clippy -p hayabusa-tools -p forensics-importer -p containment-engine --all-targets -- -D warnings

cd ..
git diff --check
```

Manual/live checks:

- disabled mode on clean install;
- Hayabusa current drop-zone smoke;
- Velociraptor offline collector fixture run;
- Velociraptor server health if enabled;
- containment shadow run with fixture critical finding;
- manual approval containment in isolated lab host;
- rollback verification;
- portal browser smoke with module disabled and with fixture findings;
- no public exposure of Velociraptor GUI unless explicitly configured.

## Codex guardrails

Codex must not:

- change Workforce core behavior while adding this module;
- re-enable heavy DLP runtime by accident;
- expose Velociraptor or Hayabusa outputs publicly;
- commit generated secrets, private configs or raw evidence;
- auto-block hosts before policy, allowlist, rollback and admin-channel checks
  exist;
- auto-block servers/domain infrastructure by default;
- claim completed integration before live/manual evidence exists;
- change Rust/API/UI runtime outside the planned files without documenting why.

Codex should:

- start with docs/config disabled mode;
- implement supply-chain pinning before service deployment;
- prefer Rust for import/validation/parsing;
- treat containment as a separate audited control plane, not as generic
  remediation;
- keep PowerShell only for Windows install/run wrappers;
- add small fixtures and negative tests before live deployment;
- update project status after each successfully verified phase.

## Expected result

After implementation AWatch-rus should have:

- installed/pinned Hayabusa and rules workflow;
- optional bundled Velociraptor server/client/offline collector modes;
- reproducible artifact pack handling for `Windows.Hayabusa.Monitoring`;
- derived findings importer into AWatch-rus forensics views;
- policy-controlled automated/manual quarantine of suspected infected
  workstations;
- rollback and emergency release path for every containment action;
- disabled-by-default safety;
- resource-bounded scans;
- clear runbooks for poor/small organizations;
- honest positioning as low-cost containment, security analytics and forensics, not
  SIEM/DLP/EDR/СЗИ.
