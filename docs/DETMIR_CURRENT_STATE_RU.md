# DetMir/AWatch-rus: актуальное состояние и граница DLP-модуля

Дата фиксации: 2026-06-25.

Обновление 2026-06-29 после восстановления RDP-сервера: фактический
production IP Windows/RDP host теперь `192.168.100.19`; stable ActivityWatch
logical host id остаётся `SHARKON2025`. Подробный post-restore baseline:
`docs/DETMIR_RESTORE_BASELINE_2026-06-29_RU.md`.

Документ фиксирует фактическое состояние DetMir/AWatch-rus и первую границу
переработки горячего пути портала. Это не release evidence для реестра
российского ПО, не заявление о сертификации и не claim замены DLP/SIEM/EDR.

## Runtime baseline

- Контур: DetMir / AWatch-rus.
- Stable Windows/RDP logical host id для ActivityWatch bucket-ов и отчетов:
  `SHARKON2025`. Это legacy logical id, а не обязательное физическое имя
  Windows-сервера. Правило rename-safe эксплуатации описано в
  `docs/WINDOWS_LOGICAL_HOST_ID_RU.md`.
- Текущий физический RDP/WinRM target: `192.168.100.19`.
- Portal host: `10.10.10.2`.
- Portal service: `detmir-portal.service`.
- Portal URL: `http://10.10.10.2:8720/`.
- Задеплоенный binary hash:
  `653b22b0fbf29a22f7de42ade7b689490b1de16fa07e785e4e0efd3078e7a3bc`.
- Бэкап предыдущего binary на сервере:
  `/usr/local/bin/detmir-portal.bak.20260625T045640Z`.
- Runtime mode после phase 1 deploy:
  `DETMIR_PORTAL_DLP_MODULE_ENABLED=false`.
- Server-side optional DLP runtime control:
  `AW_DLP_ENABLED=false|true` и `DETMIR_DLP_ENABLED=false|true`.
- Runtime control/statistics script:
  `scripts/detmir_dlp_runtime_control.sh` / live
  `/usr/local/bin/detmir-dlp-runtime-control`.
- Live DLP runtime state after 2026-06-25 controlled disable:
  `AW_DLP_ENABLED=false`, `AW_DLP_INFLUX_ENABLED=false`;
  active/enabled DLP units: `0/0`.
- Reason: DLP runtime materially increases Proxmox VM/LXC, InfluxDB, Grafana,
  ClickHouse and AW server load. In production DetMir it is currently kept
  disabled, but remains a documented optional module that can be enabled later.
- Health после деплоя: `/healthz` возвращал `status=ok`.
- Readiness после деплоя: `/readyz` возвращал `status=ready`.

## Что исправлено в текущем baseline

- Первичная загрузка портала больше не зависает в бесконечном `LOADING`.
- При холодном старте тяжелый операторский срез не блокирует UI бесконечно.
- Frontend показывает честное состояние `STALE / Первичный срез прогревается`.
- При prewarm больше не смешиваются статусы `STALE` и ложное
  `Данные отсутствуют`.
- Progress bar доходит до `100%` в fail-soft/prewarm состоянии.
- Browser smoke после деплоя подтвердил:
  - `loadStatus=STALE`;
  - `progress=100%`;
  - `LOADING=false`;
  - `EMPTY=false`;
  - `ERROR=false`.

## Phase 1 live verification

2026-06-25 после сборки `detmir-portal` и деплоя через
`ansible/deploy_detmir_portal.yml --limit proxmox` подтверждено:

- `/healthz`: `status=ok`;
- `/readyz`: `status=ready`;
- `/api/reports`: `ok=true`, `cache_status=warming`,
  `modules.dlp.enabled=false`, `modules.dlp.hot_path=false`;
- `/api/operator`: `cache_status=warming`, `summary.severity=STALE`,
  `modules.dlp.status=disabled`, `incidents=0`;
- server log для `/api/operator`: `status=200`, `latency_ms=49`.

Это подтверждает, что первый operator/API screen больше не блокируется на
холодной полной сборке. Полный snapshot строится в фоне и честно помечается как
`warming`/`STALE`.

## Текущая проблема производительности

Наблюдаемые признаки:

- после restart полный тяжелый snapshot может уходить в prewarm/stale mode;
- report cache и stale UI защищают пользователя от зависания, но не убирают
  саму стоимость тяжелых расчетов;
- DLP evidence, screenshots, endpoint signals, case review и forensics
  enrichment требуют больше CPU/IO/сетевых операций, чем Workforce core.

Вывод: DLP/evidence/forensics enrichment уже вынесен из обязательного hot path
phase 1 через `DETMIR_PORTAL_DLP_MODULE_ENABLED=false`, но полная оптимизация
тяжелого snapshot/prewarm остается отдельной инженерной задачей.

## Целевая граница после переработки

Core hot path:

- Workforce Operations;
- worktime/activity;
- загрузка, простои, перегруз;
- дисциплина процесса;
- качество и полнота данных;
- легкие агрегаты для руководителя;
- readiness/health без ожидания DLP.

Optional DLP module:

- DLP endpoint signals;
- clipboard/USB/print/web/file operation incidents;
- screenshots/evidence;
- DLP/case review;
- heavy security correlation;
- forensics timelines and evidence packages;
- Hayabusa enrichment where needed.

Отключение optional DLP module не должно ломать:

- `/healthz`;
- `/readyz`;
- первичную загрузку портала;
- Workforce views;
- management reports;
- базовый operator dashboard.

Отключение optional DLP module должно показывать честный статус:

```text
DLP module disabled / not configured
```

без заявления, что DLP-проверки выполнены.

## Рекомендуемые feature flags

Минимальная целевая модель конфигурации:

```json
{
  "modules": {
    "dlp": {
      "enabled": false,
      "evidence": false,
      "correlation": false,
      "screenshots": false,
      "hot_path": false
    }
  }
}
```

Первая реализованная runtime-граница:

```text
--dlp-module-enabled
DETMIR_PORTAL_DLP_MODULE_ENABLED=true|false
AW_DLP_ENABLED=true|false
DETMIR_DLP_ENABLED=true|false
```

Default остается `true`, чтобы существующее поведение не менялось без явного
решения администратора. Для ускоренного Workforce/operator режима допускается
`DETMIR_PORTAL_DLP_MODULE_ENABLED=false`; в этом режиме портал:

- не читает DLP incident/case/review/audit файлы в основном report/operator
  path;
- отключает security-events backend внутри snapshot, не меняя сохраненные
  ClickHouse credentials;
- возвращает disabled-state для DLP evidence API;
- не считает отсутствие DLP ошибкой Workforce core.

Ansible-параметр поставки:

```yaml
detmir_portal_dlp_module_enabled_override: false
```

Отдельный `detmir-portal-evidence` сервис не отключается этим флагом и остается
самостоятельным контуром evidence/API при наличии отдельной конфигурации.

Hayabusa/Velociraptor boundary:

- Hayabusa/Sigma and Velociraptor are optional security findings / forensics
  sources, not Workforce hot path dependencies.
- Heavy DLP runtime can remain disabled while Hayabusa/Velociraptor findings
  are imported into Security Finding Inbox / ClickHouse.
- Velociraptor server/client mode must be enabled explicitly
  (`disabled|offline_collector|server_clients`) and must not be auto-started by
  routine production deploy on the small DetMir Proxmox contour.
- Findings from Velociraptor/Hayabusa support `decide -> plan -> approve ->
  apply -> verify`, but do not imply automatic remediation without approval.

Server-side optional DLP runtime описан отдельно:

- [DLP_OPTIONAL_RUNTIME_RU.md](DLP_OPTIONAL_RUNTIME_RU.md).

При `AW_DLP_ENABLED=false`:

- `dlp-health-check` возвращает штатный `dlp:mode=disabled`;
- `detmir-dlp` не выполняет SSH health probe;
- `detmir-check`, `check-aw-full` и `check-aw-data` не считают DLP buckets
  обязательными;
- `detmir-readiness` не требует DLP Influx write и DLP systemd units;
- перед отключением и после отключения собираются JSON-срезы в
  `/var/lib/activitywatch/health/dlp-runtime-history/`, latest-срез остается в
  `/var/lib/activitywatch/health/dlp-runtime-state.json`.

Live disable evidence 2026-06-25:

- `dlp-health-check` returned `ok=true`, `dlp:mode=disabled`;
- `detmir-dlp` returned `ok=true`, `dlp:mode=disabled`;
- pre-disable active units:
  `aw-dlp-influx-exporter.timer`,
  `activitywatch-dlp-aggregator.timer`,
  `aw-dlp-report-scheduler.timer`,
  `aw-dlp-syslog-forwarder.timer`,
  `aw-dlp-webhook-sender.timer`,
  `aw-dlp-cef-exporter.timer`,
  `aw-dlp-ioc-refresh.timer`,
  `aw-dlp-policy-engine.service`,
  `aw-dlp-case-management.service`,
  `detmir-portal-evidence.service`;
- post-disable active/enabled DLP units: `0/0`;
- retained evidence files:
  `/var/lib/activitywatch/health/dlp-runtime-history/dlp-runtime-current-20260625T083619Z.json`,
  `/var/lib/activitywatch/health/dlp-runtime-history/dlp-runtime-pre_disable-20260625T083637Z.json`,
  `/var/lib/activitywatch/health/dlp-runtime-history/dlp-runtime-disabled-20260625T083700Z.json`;
- `check-aw-full` with `AW_DLP_ENABLED=false` reports `DLP buckets ...
  SKIPPED`;
- ActivityWatch core services remained active:
  `activitywatch-server`, `aw-worktime-api`.

Separate live observations after DLP disable, before 2026-06-29 restore:

- `aw-watcher-afk`, `aw-watcher-window` and `aw-worktime-sessions` were stale
  in the manual check and require separate RDP collector/session recovery;
- WinRM from the server side to `192.168.100.18:5985` was unreachable during
  this check;
- these are not treated as DLP disable regressions.

Post-restore correction on 2026-06-29:

- current physical RDP/WinRM target is `192.168.100.19`;
- laptop route to `192.168.100.19` is through DetMir OpenVPN gateway
  `10.0.13.1`;
- WinRM `5985` and RDP `3389` are reachable from the admin laptop;
- stable ActivityWatch logical host id remains `SHARKON2025`.

Fail-safe правила:

- если DLP выключена, Security/Forensics views должны показывать disabled-state,
  а не падать;
- если DLP включена, тяжелые DLP операции должны выполняться асинхронно или
  через cache, а не блокировать первый Workforce/operator screen;
- readiness не должен заявлять DLP healthy, если модуль отключен или не
  проверен;
- отсутствие DLP не является ошибкой Workforce core.

## Сетевое состояние

Доступ к DetMir зависит от NetworkManager VPN profile
`pfSense-gate-UDP4-1194-vpn_prog10-config`. Имя tun-интерфейса не является
семантической идентичностью и должно проверяться по адресу `10.0.13.*`.

Пример рабочего route:

```text
10.10.10.2 via 10.0.13.1 dev <current-detmir-tun>
```

Наблюдавшаяся нестабильность dataplane:

- tun device может присутствовать в routing table, но gateway `10.0.13.1` не
  отвечает;
- при этом SSH, `/healthz` и браузерная проверка DetMir недоступны;
- после ручного поднятия NetworkManager-подключения
  `pfSense-gate-UDP4-1194-vpn_prog10-config` доступ восстанавливался.
- 2026-06-25 после phase 1 deploy зафиксирован отдельный сбой:
  `nm-openvpn` для `178.178.98.83:1194` получил `TLS handshake failed` и
  `connect timeout exceeded`; повторная production-очистка старого systemd
  prewarm drop-in отложена до восстановления VPN handshake.

Актуальная проверка 2026-06-29:

```text
192.168.100.19 via 10.0.13.1 dev <current-detmir-tun>
10.0.13.1 ping OK
192.168.100.19:5985 OK
192.168.100.19:3389 OK
```

Команда восстановления:

```bash
nmcli connection down 'pfSense-gate-UDP4-1194-vpn_prog10-config' || true
sleep 2
nmcli connection up 'pfSense-gate-UDP4-1194-vpn_prog10-config'
```

Проверка:

```bash
ping -c 2 -W 2 10.0.13.1
ping -c 2 -W 2 10.10.10.2
nc -vz -w 3 10.10.10.2 22
curl -sS --max-time 5 http://10.10.10.2:8720/healthz
```

## Что не менять без отдельной задачи

- Не удалять DLP collectors и warehouse ради ускорения портала.
- Не включать heavy DLP или Velociraptor server runtime автоматически при
  обычном deploy без ресурсного решения.
- Не менять UI/API несовместимо: новые поля должны быть additive.
- Не заявлять completed DLP decoupling до live deploy и browser/API smoke.
- Не позиционировать AWatch-rus как сертифицированную DLP/SIEM/EDR/СЗИ.

## Следующий инженерный шаг

Закрыть оставшиеся production-hardening пункты:

1. После восстановления DetMir VPN повторно прогнать
   `ansible/deploy_detmir_portal.yml`, чтобы удалить legacy drop-in
   `/etc/systemd/system/detmir-portal.service.d/30-prewarm-after-start.conf`.
2. Подтвердить remote `sha256sum /usr/local/bin/detmir-portal` и отсутствие
   `ExecStartPost` prewarm в `systemctl cat detmir-portal`.
3. Подтвердить browser smoke без зависания первичной загрузки.
4. Добавить метрики и smoke для режимов `dlp.enabled=false` и
   `dlp.enabled=true`.
