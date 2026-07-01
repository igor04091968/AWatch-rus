# DetMir/AWatch-rus: hardline resilience hardening

Документ фиксирует реализованные и проверенные шаги по доведению живого
контура до fail-closed уровня. Он не заменяет production runbook; здесь только
изменения, влияющие на отказоустойчивость.

## 2026-06-30: crash-test readiness gate and healthd route boundary

Статус: implemented in repo, deployed on live AW server, verified by manual
crash test.

Что прогонялось:

- baseline `check-aw-full.sh`, SQLite hot-path plan, disk/headroom, RDP guard;
- bounded parallel load на `/api/0/info`,
  `/aw-worktime-sessions_SHARKON2025/events?limit=100`,
  `aw-detmir-web-category_SHARKON2025` и Worktime API;
- controlled restart: `aw-worktime-api`, `activitywatch-server`,
  `AWatchRusCollectorGuard`;
- gateway/ClickHouse/Grafana reachability checks;
- live `scripts/detmir_resilience_check.sh --live` on AW server.

Что найдено:

- `systemctl is-active activitywatch-server` не равен полной готовности API:
  сразу после `systemctl restart activitywatch-server` первый `/api/0/info`
  мог уйти в 15 секунд timeout, затем API стабилизировался и hot-path
  `/events?limit=100` отвечал быстро;
- `aw-dlp-case-management.service` оставался active при disabled DLP profile;
- `aw-rus-healthd.service` падал из-за TCP timeout с AW server до
  `192.168.100.19:5985/3389`, хотя фактическая RDP/WinRM проверка с
  admin/VPN side и bucket freshness были зелёными;
- SQLite hot-path index
  `events_bucketrow_starttime_desc_index` присутствовал и использовался,
  `TEMP B-TREE` для worktime event query не строился.

Что изменено:

- `scripts/detmir_resilience_check.sh --live` теперь использует readiness-loop
  для `/api/0/info`, проверяет worktime hot path, Worktime API rows/degraded
  state, SQLite hot-path index/plan и disabled-state optional DLP/Loki units;
- `aw-rus-healthd-rust` получил fail-closed параметр
  `AW_RUS_HEALTH_RDP_TCP_REQUIRED` / `--rdp-tcp-required`;
- default/example остаётся `true`; в DetMir production выставлено
  `false`, потому что server-side TCP до RDP сейчас является route/ACL
  boundary, а не authoritative proof of collector health;
- active drift `aw-dlp-case-management.service` остановлен, unit оставлен
  disabled для штатного будущего включения DLP contour.

Live verification:

- `check-aw-full.sh`: `FRESH=8`, `STALE=0`, `DEAD=0`;
- targeted load after stabilization:
  `info_p2/p4/p8/p12` по `24/24` HTTP 200,
  worktime events `40/40` HTTP 200,
  web category bucket `40/40` HTTP 200,
  Worktime today `30/30` HTTP 200;
- `AWatchRusCollectorGuard` restart: service `Running`, `GUARD_CHILDREN=1`,
  collector process layout unchanged;
- `aw-rus-healthd.service`: `status=0/SUCCESS`, `ok=11`, `warn=3`, `fail=0`;
- `scripts/detmir_resilience_check.sh --live` after fixes:
  readiness, hot path, Worktime API, SQLite index, optional DLP and Loki checks
  pass; Hayabusa quarantine warning remains informational evidence to review.

Safety guardrails:

- no AW bucket schema, API, UI or Workforce business logic changed;
- GitHub/Grafana/ClickHouse are still validation/visibility surfaces, not
  Russian registry release evidence;
- DLP remains optional/reconnectable, not removed.

## 2026-06-25: optional DLP runtime off switch and statistics

Статус: implemented in repo, deployed, live disable verified on 2026-06-25.

Проблема:

- DLP runtime может создавать избыточную нагрузку на InfluxDB, Grafana,
  ClickHouse и AW server при включенных aggregator/exporter/case/report
  pipeline;
- простая остановка DLP units раньше приводила бы к ложным красным health,
  readiness и contour checks.

Что добавлено:

- `AW_DLP_ENABLED=false` для AW server runtime;
- `DETMIR_DLP_ENABLED=false` для управляющего DetMir contour check;
- `dlp-health-check` возвращает штатный `dlp:mode=disabled`;
- `detmir-dlp` не выполняет SSH health probe при disabled mode;
- `detmir-check`, `check-aw-full`, `check-aw-data` пропускают DLP buckets при
  disabled mode;
- `detmir-readiness` не требует DLP Influx write и DLP systemd units при
  disabled mode;
- `scripts/detmir_dlp_runtime_control.sh` собирает JSON-срез DLP units/buckets
  и выполняет controlled `disable|enable`.
- live `disable` сохраняет отдельные evidence-снимки `current`,
  `pre_disable` и `disabled` в
  `/var/lib/activitywatch/health/dlp-runtime-history/`.

Safety guardrails:

- ActivityWatch server, worktime, Hayabusa, 1C/ClickHouse core не отключаются;
- historical DLP buckets/evidence не удаляются;
- disabled-state не заявляет, что DLP проверки выполнены;
- это не claim замены DLP/SIEM/EDR и не удаление DLP-функциональности.

Runbook:

- [DLP_OPTIONAL_RUNTIME_RU.md](DLP_OPTIONAL_RUNTIME_RU.md).

Live verification 2026-06-25:

- before disable, active DLP runtime units were present:
  `aw-dlp-influx-exporter.timer`, `activitywatch-dlp-aggregator.timer`,
  DLP report/integration timers, policy/case services and
  `detmir-portal-evidence.service`;
- after disable, active/enabled DLP units: `0/0`;
- `AW_DLP_ENABLED=false`, `AW_DLP_INFLUX_ENABLED=false`,
  `AW_DLP_DISABLED_REASON=operator_disabled_to_reduce_influx_grafana_clickhouse_load`;
- `dlp-health-check` and `detmir-dlp` both returned `dlp:mode=disabled`;
- `check-aw-full` reported DLP buckets as `SKIPPED`;
- ActivityWatch core remained active:
  `activitywatch-server`, `aw-worktime-api`.

Residual non-DLP findings from the same check:

- RDP-side collectors require separate recovery: AFK/window/worktime buckets
  were stale;
- server-side WinRM reachability to `192.168.100.18:5985` was unavailable;
- `aw-rus-healthd.service` was already failed and is tracked separately from
  this DLP runtime disable.

## 2026-06-24: Hayabusa poison-package isolation

Статус: implemented locally, unit-tested, deployed on AW server.

Проблема:

- один битый zip в `/opt/hayabusa/inbox/incoming` мог остановить весь
  Hayabusa pipeline;
- `aw-hayabusa-drop.path` после повторных падений мог упереться в systemd
  start-limit;
- восстановление требовало ручного переноса bad zip в quarantine.

Что изменено:

- `aw-hayabusa-autoprocess-rust` проверяет drop zip до `accept`;
- corrupt/empty/unsafe zip и битые sidecar-файлы не попадают в рабочий inbox;
- bad drop package переносится в `/opt/hayabusa/quarantine/drop/...` вместе с
  `.meta.json`, `.caseid`, optional `.sha256` и `reason.json`;
- `aw-hayabusa process-inbox` больше не abort'ит весь batch из-за одного
  incoming package;
- failed incoming package переносится в
  `/opt/hayabusa/quarantine/incoming/...` с partial staging payload и
  `reason.json`, если пакет остался в incoming;
- пакеты, уже архивированные wrapper'ом как `failed-no-evtx` или
  `failed-analysis`, остаются в штатном archive/intake manifest для
  расследования.

Safety guardrails:

- quarantine не удаляет evidence;
- replay выполняется только после re-export или явного восстановления пакета;
- один poison archive не должен мешать обработке остальных zip;
- operational failures инфраструктуры (`aw-hayabusa` отсутствует, права,
  broken runtime) остаются красными и не маскируются как успешная обработка.

Проверки:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
bash -n aw-server/hayabusa/aw-hayabusa.sh

cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/adk-rust
export CARGO_TARGET_DIR=/home/igor/.cache/detmir-adk-rust-target
cargo test -p hayabusa-tools
```

Результат проверки:

- `bash -n aw-server/hayabusa/aw-hayabusa.sh` passed;
- `cargo test -p hayabusa-tools` passed: 5 tests passed.

Production deployment note:

- после сборки и доставки нового `aw-hayabusa-autoprocess-rust` нужно
  выполнить live dry-run на empty drop и контролируемый bad-zip test в
  непроизводственном каталоге или с временным isolated `--drop-dir`;
- live production queue руками не мутировать без предварительного backup/listing.

Live rollout evidence:

- deployed on AW server on `2026-06-24`;
- previous `/usr/local/bin/aw-hayabusa` and
  `/usr/local/bin/aw-hayabusa-autoprocess-rust` were backed up with timestamp
  suffix;
- `/usr/local/bin/aw-hayabusa doctor` returned OK;
- isolated empty-drop dry-run returned `no zip packages in drop dir`;
- production queue after rollout: `incoming_zip=0`, `DROP_COUNT=0`,
  `aw-hayabusa-drop.path=active`, `aw-hayabusa-drop.service=inactive`;
- stale staging residue from `2026-06-20` was moved, not deleted, to
  `/opt/hayabusa/quarantine/staging-stale-20260624T190739Z/` with
  `reason.json`;
- after cleanup: `staged_dirs=0`, `archived_packages=74`,
  `archived_payloads=74`.

## 2026-06-24: Windows collector guard service child watchdog

Статус: implemented locally, static checks passed, deployed on RDP host.

Проблема:

- Windows service `AWatchRusCollectorGuard` мог оставаться в состоянии
  `running`, когда дочерний `aw-windows-telemetry.exe collector-guard` уже
  отсутствовал;
- SCM recovery не срабатывал, потому что сам service wrapper не падал.

Что изменено:

- `AWatchRusCollectorGuardService.cs` теперь подписывается на `Process.Exited`;
- при неожиданном выходе child-процесса wrapper делает bounded restart;
- restart budget: 5 child restarts за 600 секунд, задержка 5 секунд;
- при исчерпании бюджета wrapper завершает service с ошибкой, чтобы Windows
  Service Control Manager применил recovery actions;
- installer включает `sc.exe failureflag <service> 1`, чтобы recovery actions
  применялись к service failures, а не только к crash-путям;
- `ActivityWatch Recovery` остаётся fallback/bootstrap задачей и не отключается.

Safety guardrails:

- штатный `Stop-Service`/shutdown выставляет `stopping=true`, поэтому child exit
  во время остановки не считается аварией;
- wrapper не меняет collector mode, bucket names, event schema и AW API;
- service-level recovery ограничен существующим `sc.exe failure` budget.

Проверки:

```powershell
pwsh -NoProfile -Command '<compile AWatchRusCollectorGuardService.cs through Add-Type>'
pwsh -NoProfile -Command '<parse install-collector-guard-service.ps1>'

powershell -NoProfile -ExecutionPolicy Bypass -File windows/install-collector-guard-service.ps1
Get-Service AWatchRusCollectorGuard
Get-Content C:\ProgramData\AWatch-rus\logs\collector-guard-service.log -Tail 20
```

Результат локальной проверки:

- `AWatchRusCollectorGuardService.cs` compiled through PowerShell `Add-Type`;
- `windows/install-collector-guard-service.ps1` parsed with PowerShell parser;
- live install/restart validation still requires Windows/RDP deployment window.

Live rollout evidence:

- deployed on RDP host on `2026-06-24`;
- previous service source, installer and exe were backed up under
  `C:\ProgramData\AWatch-rus\backup\collector-guard-service-20260624T190827Z`;
- installer completed with `Runtime: rust`, `Mode: enforce`;
- SCM `failureflag` is enabled:
  `FAILURE_ACTIONS_ON_NONCRASH_FAILURES: TRUE`;
- controlled fault-injection killed only the child
  `aw-windows-telemetry.exe collector-guard`;
- wrapper observed child exit, attempted bounded restarts, SCM recovery restarted
  wrapper after budget exhaustion, and child stabilized;
- final validation after one guard loop: service `Running`, `CHILD_COUNT=1`,
  child `aw-windows-telemetry.exe`, latest rust guard cycle `status=ok`.

Live validation после деплоя:

- убить только child `aw-windows-telemetry.exe collector-guard`;
- убедиться, что service остаётся running и child перезапущен;
- повторить child crash больше 5 раз за 600 секунд в тестовом окне;
- убедиться, что service перешёл через SCM recovery, а не остался
  `running/no child`.

## 2026-06-24: contour resilience check

Статус: implemented locally, shell syntax/repo-mode passed, live AW server
check passed.

Проблема:

- отдельные исправления легко потерять при deploy/drift;
- CI не проверял, что poison-package isolation и child watchdog реально
  присутствуют в коде и документации;
- live-проверки должны оставаться read-only и не маскировать production сбой.

Что добавлено:

- `scripts/detmir_resilience_check.sh`;
- `--repo` режим для CI-safe проверки hardening-файлов, паттернов и docs;
- `--live` режим для read-only проверки локального AW/Hayabusa host:
  `activitywatch-server`, `aw-worktime-api`, AW `/api/0/info`, failed systemd
  units, Hayabusa `incoming/drop/quarantine`, SQLite DB/WAL size;
- `RUN_RESILIENCE_CHECK=1` hook в `scripts/run_awatch_contour_check.sh`;
- `DETMIR_RESILIENCE_STRICT_SECRETS=1` режим, который fail'ит literal
  `ansible_password`/`ansible_become_password` в private inventory без вывода
  значений.

Safety guardrails:

- check не рестартует сервисы, не двигает очереди, не пишет в production dirs;
- secret check печатает только факт наличия literal assignments, не значения;
- live mode запускается явно через `--live` или `--all`;
- GitHub/public CI может использовать только `--repo`.

Secret handling update 2026-06-30:

- literal `ansible_password` и `ansible_become_password` удалены из локального
  `ansible/inventory.ini`;
- `aw_server` читает SSH/sudo secrets из `AW_SSH_PASSWORD` и
  `AW_SUDO_PASSWORD`;
- `proxmox` читает SSH/sudo secrets из `AW_PROXMOX_SSH_PASSWORD` и
  `AW_PROXMOX_SUDO_PASSWORD`, с fallback на `AW_SSH_PASSWORD` /
  `AW_SUDO_PASSWORD`;
- `aw_windows` читает WinRM secret из `AW_WINRM_PASSWORD`;
- `inventory.example.ini` больше не содержит placeholder password-поля.

Проверки:

```bash
bash -n scripts/detmir_resilience_check.sh
bash scripts/detmir_resilience_check.sh --repo
```

Результат локальной проверки:

- shell syntax passed for `scripts/detmir_resilience_check.sh`;
- shell syntax passed for `scripts/run_awatch_contour_check.sh`;
- repo-mode passed with `ok=15`, `fail=0`;
- repo-mode reported one WARN: literal Ansible password assignments appear to
  exist in `ansible/inventory.ini`; values are not printed, and
  `DETMIR_RESILIENCE_STRICT_SECRETS=1` converts this to fail for private
  contour gates.

Live AW server result:

- `bash /tmp/detmir_resilience_check.sh --live` passed on AW server;
- result: `ok=9`, `warn=1`, `fail=0`;
- WARN is expected after this rollout: one quarantine `reason.json` exists for
  the moved stale Hayabusa staging residue.

## 2026-06-24: live drift fixes after full contour re-check

Статус: deployed and verified live.

Что было найдено:

- ClickHouse container was healthy by direct SQL checks, but Docker Compose did
  not define a container `HEALTHCHECK`; because of that
  `aw-1c-clickhouse-health.service` failed with
  `docker healthcheck is not configured`.
- `detmir-auto` used the default public gateway URL for portal checks when
  `/etc/detmir/detmir-check.env` did not set `DETMIR_PORTAL_URL`; protected
  public `/readyz`, `/version` and `/metrics` returned legitimate `401`.
- `detmir-portal-prewarm.service` had `curl --max-time 60`, but a cold
  `/api/reports` build on the live contour can take more than 60 seconds.
- `aw-rus-healthd-rust` used the default 20 second wrapper timeout; under
  concurrent checks `dlp-health-check --json` could be killed mid-output and be
  reported as `invalid JSON output`.

Что изменено:

- `clickhouse-1c/docker-compose.yml` now defines a ClickHouse client
  `HEALTHCHECK`, deployed to `/opt/activitywatch/clickhouse-1c/docker-compose.yml`;
- production `/etc/detmir/detmir-check.env` now contains
  `DETMIR_PORTAL_URL=http://127.0.0.1:8720` and
  `DETMIR_GATEWAY_HOST=127.0.0.1`;
- `ops/systemd/detmir-portal-prewarm.service` is now tracked in the repo and
  deployed with `curl --max-time 180` and `TimeoutStartSec=210`;
- production `/etc/activitywatch/aw-server.env` and
  `aw-server/aw-server.env.example` now set
  `AW_RUS_HEALTH_WRAPPER_TIMEOUT_SECONDS=90`.

Verification:

- `check-aw-full.sh`: `FRESH=8`, `STALE=0`, `DEAD=0`;
- `detmir-check` through the production env file: `ok=true`,
  `service_failures=0`;
- `detmir-auto.service`, `awatch-contour-daily-check.service`,
  `awatch-contour-weekly-check.service`: `status=0/SUCCESS`;
- `detmir-portal-prewarm.service`: `status=0/SUCCESS`;
- `aw-1c-clickhouse-health.service`: `status=0/SUCCESS`, Docker state
  `(healthy)`;
- `aw-rus-healthd.service`: `status=0/SUCCESS`, failed systemd units on
  AW server and Proxmox are zero.

## 2026-06-25: portal cold-start prewarm after service restart

Статус: deployed and verified live at the time, then superseded by the
fail-soft hot-path boundary below.

Что было найдено:

- после ручного `systemctl restart detmir-portal` первый
  `/api/reports?role=manager` может выполнять холодный расчет дольше 120 секунд;
- `detmir-portal-prewarm.timer` держит cache теплым каждые 30 минут, но не
  запускается немедленно при ручном рестарте портала.
- одного prewarm недостаточно, если каждый пользовательский `/api/reports`
  заново запускает тяжелую генерацию отчета.

Что изменено:

- добавлен tracked drop-in
  `ops/systemd/detmir-portal.service.d/30-prewarm-after-start.conf`;
- production drop-in `/etc/systemd/system/detmir-portal.service.d/30-prewarm-after-start.conf`
  запускает `detmir-portal-prewarm.service` через `systemctl --no-block` после
  каждого старта портала;
- prewarm остается best-effort: портал стартует независимо, а тяжелый
  `/api/reports` прогревается в фоне.
- в `detmir-portal` добавлен short-lived in-process report cache с TTL 120
  секунд и защитой от stampede: первый report-запрос строит payload, следующие
  report endpoints в окне TTL отдают тот же payload без повторной генерации.
- в `/metrics` добавлены отдельные счетчики report-cache:
  `awatch_report_requests_total`, `awatch_report_cache_hits_total`,
  `awatch_report_cache_misses_total`. Старый
  `awatch_reports_generated_total` остается счетчиком успешно завершенных
  тяжелых генераций отчета, а не счетчиком HTTP-запросов.

Verification:

- `/healthz`: `200`;
- `/readyz`: `200`, `status=ready`;
- prewarm after restart: `status=0/SUCCESS`;
- cold `/api/reports?role=manager` after expired cache: `200`, около `63s`;
- warm `/api/reports?role=manager`: `200`, около `0.34..0.35s` в трех
  последовательных запросах;
- `awatch_reports_generated_total` не вырос после трех warm report-запросов;
- `awatch_report_requests_total` растет на report endpoints, а
  `awatch_report_cache_hits_total` растет на warm cache-запросах;
- во время cold/prewarm сборки `awatch_report_requests_total` показывает
  входящие report-запросы до ожидания cache lock, а
  `awatch_reports_generated_total` растет только после готового payload;
- `workforce_operations.summary` и `workforce_operations.rows` доступны в JSON;
- browser smoke: блок `Операционная загрузка` отрисован, строки сотрудников
  видны, console errors/warnings отсутствуют.

Superseded note:

- restart-triggered external prewarm reduced warm-cache latency, but it also
  kept a heavy full-report job coupled to service restart;
- after the DLP/hot-path phase 1 change, the preferred production behavior is
  immediate `warming`/`STALE` API response from the portal itself, not a
  mandatory heavy `ExecStartPost` prewarm after every restart;
- legacy drop-ins
  `/etc/systemd/system/detmir-portal.service.d/20-prod-timeout.conf` and
  `/etc/systemd/system/detmir-portal.service.d/30-prewarm-after-start.conf`
  are now treated as stale deployment residue and are removed by
  `ansible/deploy_detmir_portal.yml`.

## 2026-06-25: current state and first DLP hot-path boundary

Статус: phase 1 implemented, targeted Rust tests passed, deployed once on the
DetMir portal host and API-smoke verified. Follow-up production cleanup of stale
restart-prewarm drop-ins is pending until DetMir VPN handshake is stable again.

Фактический runtime:

- production `detmir-portal` binary:
  `653b22b0fbf29a22f7de42ade7b689490b1de16fa07e785e4e0efd3078e7a3bc`;
- deploy command used:
  `ansible-playbook -i inventory.ini deploy_detmir_portal.yml --limit proxmox -e detmir_portal_bind_override=0.0.0.0:8720 -e detmir_portal_dlp_module_enabled_override=false`;
- `/healthz`: `status=ok` after deploy;
- `/readyz`: `status=ready` after deploy;
- `/api/reports`: `ok=true`, `cache_status=warming`,
  `modules.dlp.enabled=false`, `modules.dlp.hot_path=false`;
- `/api/operator`: `cache_status=warming`, `summary.severity=STALE`,
  `modules.dlp.status=disabled`, `incidents=0`;
- server log: `/api/operator` returned `200` with `latency_ms=49`;
- browser smoke after restart: `loadStatus=STALE`, progress `100%`,
  `LOADING=false`, `EMPTY=false`, `ERROR=false`.

Что это означает:

- первичное зависание портала устранено на уровне UX/cache/stale fallback;
- `/api/operator` no longer waits for the cold full snapshot and can return a
  bounded `warming` payload;
- тяжелая генерация полного отчета/snapshot все еще может быть дорогой во время
  cold/prewarm;
- DLP/security enrichment has a first runtime boundary out of the Workforce hot
  path: phase 1 used `DETMIR_PORTAL_DLP_MODULE_ENABLED=false`; the current
  DetMir production default keeps DLP runtime disabled/`core_only`, while
  `light` remains an explicit operator re-enable profile after resource check;
- текущее состояние зафиксировано отдельно:
  `docs/DETMIR_CURRENT_STATE_RU.md`.

Архитектурное решение для следующего шага:

- Workforce core должен оставаться быстрым и доступным без DLP;
- DLP evidence, endpoint signals, screenshots, case review, heavy correlation
  and forensics enrichment должны стать optional module;
- prewarm не должен обязательно выполнять heavy DLP path;
- Security/Forensics views при отключенной DLP должны показывать disabled-state,
  а не ломать portal readiness.

Реализованная первая граница:

- CLI/env flag: `--dlp-module-enabled` /
  `DETMIR_PORTAL_DLP_MODULE_ENABLED`;
- DetMir production default после resource hardening: `false` / `core_only`,
  чтобы обычный deploy/recovery не возвращал DLP нагрузку на Proxmox, AW,
  ClickHouse, InfluxDB и Grafana;
- `light` допускается только как явное operator re-enable действие после
  resource check; при `light` основной portal snapshot не читает тяжелые
  incident/case/review/audit DLP state, а evidence/case/exporter path остается
  выключенным;
- Ansible deploy parameter:
  `detmir_portal_dlp_module_enabled_override`.

Проверено локально:

- `cargo test -p detmir-portal --locked`;
- `cargo clippy -p detmir-portal --all-targets --locked -- -D warnings`.

Deployment cleanup status:

- `ansible/deploy_detmir_portal.yml` now removes stale restart-prewarm drop-ins:
  `20-prod-timeout.conf` and `30-prewarm-after-start.conf`;
- repeat production deploy of that cleanup is pending because the DetMir
  `pfSense-gate-UDP4-1194-vpn_prog10-config` tunnel later failed TLS handshake
  to `178.178.98.83:1194`;
- do not claim final production prewarm cleanup until
  `systemctl cat detmir-portal` no longer shows `ExecStartPost` prewarm.

Ограничения:

- это не удаление DLP collectors;
- это не claim, что production DLP decoupling уже завершен без live smoke;
- это не registry release evidence;
- GitHub/GitHub Actions не являются primary registry build contour.

## 2026-06-30: DLP disabled/core_only default, load guard and rollback

Статус: implemented in repository defaults/scripts/docs, deployed on live
DetMir contour and verified manually.

Что изменено:

- DetMir production defaults переведены в `AW_DLP_ENABLED=false` и
  `AW_DLP_PROFILE=core_only`;
- `aw_dlp_enabled=false`, `aw_dlp_influx_enabled=false`;
- lightweight collector остается подключаемым, но не стартует по умолчанию;
- heavy DLP component flags в production defaults остаются выключены;
- `detmir_portal_dlp_module_enabled_override=false` показывает честный
  disabled-state в портале без heavy evidence/case path;
- добавлен `scripts/detmir_dlp_load_guard.sh`;
- `detmir-dlp-load-guard.timer` контролирует load/RAM/iowait и при перегрузе
  переводит DLP в `core_only`;
- Ansible DLP tasks больше не имеют DLP-heavy default `true`;
- `scripts/detmir_dlp_runtime_control.sh` получил профили
  `core_only`, `light`, `on_demand`, `full`;
- перед каждым `set-profile` сохраняется rollback-снимок systemd
  active/enabled состояния DLP units;
- `rollback` восстанавливает предыдущее состояние DLP units без изменения
  retention и без запуска Loki CT.

Эксплуатационная позиция:

- Loki CT отключён намеренно для снижения нагрузки на Proxmox VM/LXC;
- Loki не является обязательной зависимостью Workforce/Worktime/AW core;
- DLP не удалён: lightweight-сбор нужен для UEBA, а тяжелый runtime не должен
  возвращаться обычным deploy/recovery;
- Hayabusa/Velociraptor findings остаются отдельным optional security layer
  через Security Finding Inbox / ClickHouse и не требуют Loki.

Runbook:

- `docs/DLP_RESOURCE_PROFILES_RU.md`;
- `docs/DLP_OPTIONAL_RUNTIME_RU.md`.

## 2026-06-24: fail-closed timeout hardening after manual live run

Статус: implemented locally, targeted Rust tests passed, deployed and verified
live.

Что было найдено ручным прогоном:

- при деградации `activitywatch-server` запросы `/api/0/buckets` и отдельные
  bucket event endpoints могли занимать 15-30 секунд;
- `detmir-check` мог зависнуть без общего дедлайна, а штатные
  daily/weekly checks оставались в `activating`;
- timeout в `detmir-check` убивал shell, но мог оставить `detmir-dlp`/`ssh`
  хвост, который удерживал stdout pipe;
- `detmir-dlp` не имел собственного SSH timeout;
- прямой `dlp-health-check --json` на AW server мог зависать дольше ожиданий;
- `aw-worktime-autoheal-rust` считал ошибкой timeout чтения response body после
  POST backfill, хотя ActivityWatch уже мог применить запись.

Что изменено:

- `detmir-check` получил общий watchdog
  `DETMIR_CHECK_OVERALL_TIMEOUT_SECONDS` и env-настройки
  `DETMIR_SERVICE_TIMEOUT_SECONDS`, `DETMIR_BUCKET_TIMEOUT_SECONDS`,
  `DETMIR_DLP_TIMEOUT_SECONDS`;
- production `/etc/detmir/detmir-check.env` настроен на:
  `DETMIR_SERVICE_TIMEOUT_SECONDS=35`,
  `DETMIR_BUCKET_TIMEOUT_SECONDS=35`,
  `DETMIR_DLP_TIMEOUT_SECONDS=120`,
  `DETMIR_CHECK_OVERALL_TIMEOUT_SECONDS=300`;
- `detmir-check` и `detmir-auto` убивают timed-out child process group, чтобы
  не оставлять shell/SSH/DLP хвосты;
- `detmir-dlp` получил bounded SSH timeout и больше не выносит SSH child в
  отдельную process group, чтобы parent timeout мог убить всю ветку;
- `dlp-health-check` получил общий self-timeout
  `AW_DLP_HEALTH_OVERALL_TIMEOUT_SECONDS` с default 120 секунд;
- `aw-worktime-autoheal-rust` для POST events проверяет HTTP status и не читает
  response body, потому что body не нужен для backfill evidence.

Safety guardrails:

- изменения не меняют AW API schema, bucket names, UI или product workflow;
- timeout failure остается красным, но больше не оставляет активные процессы и
  lock poisoning;
- production timeouts расширены только до фактической live latency, общий
  deadline остается bounded;
- remote DLP и worktime autoheal не публикуют secrets/PII в logs сверх уже
  существующих operational identifiers.

Проверки:

```bash
cargo test --manifest-path adk-rust/Cargo.toml \
  -p detmir-check -p detmir-auto -p detmir-dlp -p dlp-health-check \
  -p worktime-autoheal

cargo build --manifest-path adk-rust/Cargo.toml --release \
  -p detmir-check -p detmir-auto -p detmir-dlp -p dlp-health-check \
  -p worktime-autoheal
```

Live verification:

- `dlp-health-check --json`: `ok=22`, `warn=0`, `fail=0`, elapsed 17s;
- `aw-worktime-autoheal.service`: success, posted `afk=28`, `win=28`;
- `aw-rus-healthd.service`: success, `ok=13`, `warn=1`, `fail=0`;
- `detmir-check` through production env: `rc=0`, elapsed 19s;
- `detmir-auto.service`: `rc=0`, elapsed 56s, bucket `dead=0`, `stale=0`,
  `ok=8`;
- `awatch-contour-daily-check.service`: `rc=0`, elapsed 14s;
- `awatch-contour-weekly-check.service`: `rc=0`, elapsed 136s;
- `check-aw-full.sh`: `FRESH=8`, `STALE=0`, `DEAD=0`;
- final AW/PVE failed systemd units: `0`;
- final RDP guard: service `Running`, 13 telemetry processes, last guard cycles
  `status=ok problems=0`.

## 2026-06-30: manual collection and analysis smoke after DLP light enablement

Статус: partially green, live fixes applied, one external reachability blocker
remains.

Ручной прогон подтвердил:

- `activitywatch-server` and `aw-worktime-api` are active;
- Worktime API `/reports/worktime/today` returns current data for 4 users;
- DetMir portal `/portal` renders through browser and no frontend console error
  was observed for the tested portal pages;
- `/api/manager` returns OK after restoring the portal timeout to 25 seconds;
- `/api/operator` returns current collection/DLP/Grafana/1C/worktime blocks;
- Security Finding Inbox is reachable for `security`/`admin` role headers and
  returns ClickHouse backend `status=ok`, `open_count=0`;
- DLP light warehouse is present on the portal host and DLP checks show OK in
  the operator card;
- Grafana backend is reachable directly at `10.10.10.11:3000/api/health`.

Live fixes applied:

- removed stale systemd drop-in
  `/etc/systemd/system/detmir-portal.service.d/10-detmir-check-env.conf`;
- restored `/etc/detmir-portal.env` timeout to
  `DETMIR_PORTAL_TIMEOUT_SECONDS=25`;
- updated `/etc/detmir/detmir-check.env` for the current light profile:
  `DETMIR_DLP_ENABLED=true` and `DETMIR_DISABLE_DLP_HEALTH_CHECK=true`;
- updated `/var/lib/detmir-ai/latest-run` to the fresh 2026-06-30
  `detmir-check` JSON so the portal no longer displays the stale 2026-06-25
  collection snapshot;
- updated Ansible to remove the stale portal timeout override during deploy.

Remaining live blocker:

- `detmir-check` still fails by design because RDP host `192.168.100.19`
  responds to ICMP, but TCP `22` and `5985` time out from the DetMir contour;
- `awatch-contour-daily-check.service` therefore remains failed with
  `service_failures=2`;
- this is not an AW-server, Worktime API, DLP warehouse, ClickHouse, or portal
  rendering failure. It is the current RDP control/reachability failure.

Grafana note:

- Browser access to Grafana through the gateway is protected by Basic Auth and
  the current certificate chain is not trusted by the Playwright browser when
  opened by IP/DNS in this run;
- direct backend health check is green:
  `http://10.10.10.11:3000/api/health -> 200`;
- gateway returns `401` without credentials, which is expected for the
  protected dashboard entrypoint.
