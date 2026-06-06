# Windows Rust validation

Дата: 2026-06-05

`aw-windows-telemetry.exe validate-deployment` является Rust-заменой первого
уровня для `windows/validate-deployment.ps1`.

Цель команды: дать машинный JSON gate перед дальнейшей заменой Windows
PowerShell runtime scripts на Rust EXE.

## Команда

```powershell
C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe `
  validate-deployment `
  --config-path C:\ProgramData\AWatch-rus\deployment-config.json
```

Вывод: JSON.

Exit codes:

- `0` - validation прошла;
- `1` - ошибка запуска, чтения config или runtime error;
- `2` - validation выполнена, но есть failed sections.

## Что проверяется сейчас

- `deployment-config.json` читается как JSON, включая UTF-8 BOM.
- `ActivityWatch File1C Upload` указывает на
  `aw-windows-telemetry.exe file1c-upload`.
- `ActivityWatch DLP Evidence Sync` указывает на
  `aw-windows-telemetry.exe dlp-evidence-sync`.
- `AWatch Rust Telemetry Agent` указывает на `awatch-agent-rs.exe`.
- `AWatchRusCollectorGuard` service указывает на
  `aw-windows-telemetry.exe collector-guard` через service wrapper.
- Rust collector guard получает `sessionId` через native Windows API,
  дедуплицирует legacy browser/fileops/DLP endpoint collectors по
  `(kind, sessionId)` и пропускает повторный запуск launch tasks, если legacy
  collectors уже работают.
- Rust worktime agent запущен.
- Rust collector guard запущен.
- Worktime bucket свежий.
- Локальные DLP/file operation queues не превышают безопасный depth.
- `aw-windows-telemetry.exe browser-domains-collector`,
  `dlp-endpoint-collector` и `file-operations-collector` доступны как P0
  runtime subcommands.
- Browser Rust collector реализует URL/domain/category parity:
  UIAutomation extraction, URL normalization, host/rootDomain, default/custom
  category rules, `aw-watcher-web-*`, `aw-detmir-web-category_*` и web DLP
  incident schema.
- DLP endpoint Rust collector реализует incident semantics для
  `clipboard_change`, `usb_insert` и `print_job`: raw endpoint signal,
  policy evaluation, cooldown и `aw-dlp-incidents_*` event fields сохранены.
  USB write-block и print cancel не выполняются в пилотном режиме без
  отдельного enforcement решения.
- В live-конфигурации `collectors.browserCollectorMode`,
  `collectors.dlpEndpointMode` и `collectors.fileOpsMode` переключены в
  `rust_primary`.
- P0 PowerShell collector runtime
  (`browser-domains-native-collector.ps1`,
  `dlp-endpoint-signals-collector.ps1`, `file-operations-collector.ps1`)
  отсутствует в штатных пользовательских сессиях.
- Rust collectors пишут per-session state/log/queue, чтобы разные RDP-сессии
  не конфликтовали за один state-файл.
- Оставшийся PowerShell runtime классифицируется для миграционной карты через
  native WinAPI process snapshot. `wmic.exe` не требуется.

## Инвентаризация процессов

На новых Windows `wmic.exe` может отсутствовать. Validator не зависит от него:

1. сначала используется native WinAPI process snapshot;
2. если native snapshot недоступен, используется `wmic.exe`;
3. если `wmic.exe` отсутствует, используется `tasklist.exe` fallback.

В штатном режиме native snapshot дает:

- `processes.commandLineQueryOk=true`;
- классификацию PowerShell runtime по видам:
  `browser`, `fileops`, `dlp_endpoint`, `guard`, `recovery`, `worktime`;
- подтверждение отсутствия `worktime-session-collector.ps1`.
- подтверждение отсутствия `aw-collector-guard.ps1`.

`tasklist.exe` fallback ограничен:

- наличие `awatch-agent-rs.exe` подтверждается;
- command line PowerShell-процессов недоступна;
- поле `processes.commandLineQueryOk=false`;
- поле `processes.noPowerShellWorktimeRuntime=null`;
- это не считается ошибкой, если task actions, Rust agent и bucket freshness
  подтверждены.

Для полного доказательства отсутствия конкретного `.ps1` процесса можно
дополнительно использовать operator inventory через WinRM/Ansible, но это не
должно блокировать Rust validation на системах без WMIC.

## Проверенный live checkpoint

2026-06-05 на Windows/RDP host:

- `overallOk=true`;
- failed sections: `[]`;
- File1C task: Rust EXE;
- DLP evidence task: Rust EXE;
- worktime agent: Rust EXE;
- collector guard service: Rust EXE child;
- worktime bucket age: меньше 300 секунд.
- native process inventory: `commandLineQueryOk=true`;
- PowerShell worktime runtime: отсутствует;
- PowerShell collector guard runtime: отсутствует;
- оставшийся PowerShell runtime классифицирован как `recovery/other`; P0
  browser/fileops/DLP endpoint PowerShell collectors отсутствуют.
- Rust browser/DLP/fileops runtime после live switch: `browser=3`,
  `dlp_endpoint=3`, `fileops=3`, `guard=1`, legacy P0 count `0`.
- После одного guard-cycle P0 Rust counts остались `3/3/3`, DLP state по трем
  сессиям: `status=ok`, `sendFailures=0`.
- После parity update 2026-06-05T23:10Z:
  - `browser_rust=3`, `dlp_endpoint_rust=3`, `fileops_rust=3`,
    `guard_rust=1`, legacy P0 collector count `0`;
  - `validate-deployment`: `overallOk=true`, failed sections `[]`;
  - AW API после restart `activitywatch-server` отвечает локально за ~0.002s;
  - `aw-detmir-web-category_*` получает Rust `collector_health` с
    `foregroundProcess`, `browserDetected`, `urlDetected`;
  - текущие RDP-сеансы disconnected, поэтому live foreground пустой и
    `browserDetected=false`, `urlDetected=false`; URL event path проверен
    self-test/unit-test, но live URL требует активного foreground browser;
  - `aw-dlp-endpoint-signals_*` получает свежие Rust `self_test` events:
    `queueDepth=0`, `sendFailures=0`;
  - `detmir-dlp`: `ok=true`, `counts={ok:22,warn:0,fail:0}`;
  - `detmir-status`: `severity=OK`, `needs_heal=false`,
    `ok_for_operator=true`; актуальный refresh через Rust-first
    `/usr/local/bin/detmir-auto --no-heal --no-report` дал
    `service_warnings=0` и DLP `counts={ok:22,warn:0,fail:0}`.
- Rust file operations live mode использует per-session queue/state/log;
  bounded shadow create/rename/delete smoke ранее подтвердил `Created`, один
  `Renamed` с `oldPath`, `Deleted` и штатные `collector_health` события.
- Collector guard duplicate prevention smoke: при принудительном stale
  action `run-task` получил `applied=false`,
  `reason=legacy-collectors-already-running`; runtime counts остались
  `browser=3`, `fileops=3`, `dlp_endpoint=2`.

Ограничение: этот checkpoint подтверждает end-to-end live runtime, policy
semantics и schema parity. Полный live URL/domain incident path физически
требует активного foreground browser в интерактивной RDP-сессии; disconnected
сеансы корректно дают только health/self-test. Удаление legacy `.ps1` остается
отдельным rollback/decommission gate.

Live hostnames, private IP и runtime report сохранены только в private
`.ops`-контуре.
