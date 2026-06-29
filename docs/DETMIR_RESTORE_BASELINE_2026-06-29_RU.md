# DetMir restore baseline 2026-06-29

Дата фиксации: 2026-06-29.

Документ фиксирует фактическое состояние восстановленного контура
AWatch-rus/DetMir после возврата RDP-сервера `SHARKON2025`.

## Итог

- AW server: `10.10.10.13:5600`, API отвечает, CORS OK.
- Worktime API: `10.10.10.13:5610`, `/health` OK, report endpoints отвечают.
- Portal/gateway: `https://dm.iri1968.dpdns.org/healthz` отвечает `200 ok`.
- RDP host physical IP: `192.168.100.19`.
- Stable ActivityWatch logical host id: `SHARKON2025`.
- Старый IP `192.168.100.18` не считать текущим production target.

## Выполненные исправления

- На ноутбуке добавлен постоянный маршрут к `192.168.100.19/32` через DetMir
  VPN gateway `10.0.13.1`.
- `ansible/inventory.ini` переведен на `rdp-prod ansible_host=192.168.100.19`.
- Диагностические скрипты больше не жёстко привязаны к `192.168.100.18`.
- `/etc/activitywatch/aw-server.env` на AW server обновлен:
  `AW_MONITORED_WINDOWS_HOST=192.168.100.19`.
- На RDP host добавлены узкие Windows Firewall allow-правила для
  `10.10.10.13 -> 5985/3389`; промежуточный firewall всё ещё блокирует этот
  server-side TCP path.
- Guard restart-budget quarantine очищен через backup и reset runtime state.
- ClickHouse Security Finding Inbox schema применена:
  `security_findings`, `security_finding_workflow_events`,
  `security_finding_inbox`.
- `check-aw-full` обновлен: env-aware RDP host, корректный CORS origin,
  AFK freshness через `bucket.metadata.end`.

## Проверенный статус по 7 пунктам

1. AW-server/portal/API: доступны; `check-aw-data.sh` OK, public `/healthz` OK.
2. RDP host: `192.168.100.19`, `COMPUTERNAME=SHARKON2025`,
   WinRM/RDP/SSH доступны с админского ноутбука; guard service running.
3. ActivityWatch buckets: worktime, session, AFK, DLP endpoint signals fresh;
   window bucket корректно классифицируется как inactive при отсутствии
   интерактивной активности.
4. ClickHouse/filter/security findings: ClickHouse healthy, Security Finding
   Inbox schema создана; portal endpoint защищен auth и без авторизации
   возвращает `401`.
5. InfluxDB/Grafana: InfluxDB health `pass`, Grafana DB health `ok`,
   Influx datasource health OK. Loki log contour intentionally disabled to
   reduce resource usage.
6. Hayabusa/Velociraptor layer: Hayabusa doctor OK, drop.path active,
   incoming/drop backlog empty, latest intake `2026-06-29T09:00:23Z`,
   bad zip сохранен только в quarantine как evidence. Velociraptor остаётся
   addon/source path; live service не заявляется как подтвержденный.
7. Baseline зафиксирован в этом документе и связан с operational docs/skills.

## Остаточные риски

- `aw-rus-healthd.service` на AW server всё ещё видит TCP timeout до
  `192.168.100.19:5985/3389` через gateway `10.10.10.1`, хотя ICMP проходит и
  доступ с админского ноутбука есть. Это network policy gap на промежуточном
  firewall/ACL, не SQLite/datastore failure.
- Grafana Loki datasource `10.10.10.12:3100` intentionally disabled: Proxmox
  LXC `202 loki-logs` is stopped, active config has `onboot: 0`, and TCP
  `10.10.10.12:3100` is closed. This is an operator decision to save resources
  and does not break core AW/worktime/ClickHouse.

## Проверки

- `AW_MONITORED_WINDOWS_HOSTNAME=SHARKON2025 ./check-aw-data.sh` - OK.
- `AW_SMOKE_AW_SERVER=http://10.10.10.13:5600
  AW_SMOKE_SOURCE_HOSTNAME=SHARKON2025
  AW_SMOKE_WINDOWS_HOST=192.168.100.19 ./check-aw-full.sh --no-color` - OK.
- `cargo test -p check-aw-full` - 4 passed.
- `bash -n` для изменённых shell scripts - OK.
- `AW_SMOKE_LOKI_ENABLED=0` is the current default for local smoke checks.
