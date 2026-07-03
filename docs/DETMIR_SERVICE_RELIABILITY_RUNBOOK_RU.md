# DetMir Service Reliability Runbook

Дата актуализации: 2026-07-03

Документ описывает production-safe контур снижения повторных выпадений сервисов
AWatch-rus/DetMir. Главная цель — отделять первичный отказ ActivityWatch API от
вторичных падений ingest/report/health jobs и выполнять ограниченное
автовосстановление только для подтвержденного безопасного сценария.

## План реализации

1. Считать `activitywatch-server` первичным сервисом для AW API.
2. Считать `aw-workforce-ingest`, `aw-worktime-autoheal`, `aw-worktime-prewarm`,
   `aw-worktime-ui-bridge`, `aw-worktime-influx-exporter`, `aw-rus-healthd` и
   `aw-slo-monitor` вторичными зависимыми jobs.
3. Проверять AW API через `/api/0/settings/` и `/api/0/buckets/`.
4. Автоматически восстанавливать только подтвержденный класс отказа:
   `poisoned datastore lock` или неактивный `activitywatch-server`.
5. Перед restart первичного сервиса временно остановить вторичные jobs, чтобы
   они не усиливали нагрузку на AW SQLite.
6. Перезапустить только `activitywatch-server` внутри CT `203`.
7. Дождаться восстановления AW API.
8. Вернуть вторичные timers и записать incident JSON.

## Что реализовано

Реализован script:

```text
scripts/detmir-aw-primary-recovery.sh
```

Systemd units для Proxmox host:

```text
ops/systemd/detmir-aw-primary-recovery.service
ops/systemd/detmir-aw-primary-recovery.timer
```

Контур предназначен для запуска на Proxmox host `10.10.10.2`, потому что там
доступны одновременно:

- управление PVE jobs через локальный `systemctl`;
- управление AW CT через `pct exec 203`;
- проверка внешнего AW API `http://10.10.10.13:5600`.

## Safety Rules

Recovery script:

- не удаляет SQLite database, lock files или journal files;
- не включает DLP, Loki или always-on Velociraptor;
- не перезапускает Windows host и Windows collectors;
- не меняет конфигурацию AW server;
- не выполняет recovery при произвольном HTTP timeout без подтвержденной
  причины;
- имеет cooldown между restart попытками;
- пишет incident evidence в JSON.

## Установка

На Proxmox host:

```bash
sudo install -m 0755 scripts/detmir-aw-primary-recovery.sh \
  /usr/local/bin/detmir-aw-primary-recovery

sudo install -m 0644 ops/systemd/detmir-aw-primary-recovery.service \
  /etc/systemd/system/detmir-aw-primary-recovery.service

sudo install -m 0644 ops/systemd/detmir-aw-primary-recovery.timer \
  /etc/systemd/system/detmir-aw-primary-recovery.timer

sudo systemctl daemon-reload
sudo systemctl enable --now detmir-aw-primary-recovery.timer
```

Опциональная конфигурация:

```bash
sudo install -d -m 0755 /etc/detmir
sudoedit /etc/detmir/aw-primary-recovery.env
```

Пример:

```bash
DETMIR_AW_RECOVERY_URL=http://10.10.10.13:5600
DETMIR_AW_RECOVERY_CT_ID=203
DETMIR_AW_RECOVERY_CONFIRM_ATTEMPTS=2
DETMIR_AW_RECOVERY_COOLDOWN_SECONDS=900
DETMIR_AW_RECOVERY_HTTP_TIMEOUT_SECONDS=8
```

## Проверка без изменений

```bash
/usr/local/bin/detmir-aw-primary-recovery --self-test
/usr/local/bin/detmir-aw-primary-recovery --check-only
systemctl start detmir-aw-primary-recovery.service
journalctl -u detmir-aw-primary-recovery.service -n 80 --no-pager
```

Ожидаемо для здорового контура:

```text
event=probe attempt=1 status=ok
```

## Evidence

Incident files:

```text
/var/lib/detmir-aw-primary-recovery/incidents/*.json
/var/lib/detmir-aw-primary-recovery/latest.json
/var/lib/detmir-aw-primary-recovery/status.json
```

Проверка последнего incident:

```bash
sudo jq . /var/lib/detmir-aw-primary-recovery/latest.json
```

Ключевые поля:

- `outcome`: `recovered`, `failed`, `skipped`, `observed_no_action`, `dry_run`;
- `reason`: `poisoned_lock`, `service_inactive`, `cooldown_active_after_*`;
- `recovered`: boolean;
- `pve_pause_units`;
- `ct_pause_units`;
- `details`.

## Rollback

Отключить automation:

```bash
sudo systemctl disable --now detmir-aw-primary-recovery.timer
sudo systemctl reset-failed detmir-aw-primary-recovery.service
```

Удаление установленных файлов не требуется для rollback. Если нужно убрать
полностью:

```bash
sudo rm -f /etc/systemd/system/detmir-aw-primary-recovery.service
sudo rm -f /etc/systemd/system/detmir-aw-primary-recovery.timer
sudo rm -f /usr/local/bin/detmir-aw-primary-recovery
sudo systemctl daemon-reload
```

## Manual Recovery Sequence

Если automation disabled или recovery не помог:

```bash
sudo systemctl stop aw-workforce-ingest.timer aw-workforce-ingest.service
sudo pct exec 203 -- systemctl stop \
  aw-worktime-autoheal.timer aw-worktime-autoheal.service \
  aw-worktime-prewarm.timer aw-worktime-prewarm.service \
  aw-worktime-ui-bridge.timer aw-worktime-ui-bridge.service \
  aw-worktime-influx-exporter.timer aw-worktime-influx-exporter.service \
  aw-rus-healthd.timer aw-rus-healthd.service \
  aw-slo-monitor.timer aw-slo-monitor.service

sudo pct exec 203 -- systemctl restart activitywatch-server

curl -fsS http://10.10.10.13:5600/api/0/settings/ >/dev/null
curl -fsS http://10.10.10.13:5600/api/0/buckets/ >/dev/null

sudo pct exec 203 -- systemctl start \
  aw-worktime-autoheal.timer \
  aw-worktime-prewarm.timer \
  aw-worktime-ui-bridge.timer \
  aw-worktime-influx-exporter.timer \
  aw-rus-healthd.timer \
  aw-slo-monitor.timer

sudo systemctl start aw-workforce-ingest.timer
```

## Post-Recovery Validation

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
./check-aw-full.sh

ssh igor@10.10.10.2 'systemctl --failed --no-legend || true'
ssh igor@10.10.10.2 'sudo pct exec 203 -- systemctl --failed --no-legend || true'

curl -fsS http://10.10.10.13:5610/health | jq .
curl -fsS http://10.10.10.2:8720/readyz | jq .
```

Критерий:

- `check-aw-full.sh`: `DEAD=0`, `STALE=0`;
- failed units на Proxmox host: `0`;
- failed units внутри CT `203`: `0`;
- Worktime health: `status=OK`;
- Portal readiness: `status=ready`.

## Gateway Contract

Операторские shortcuts должны соответствовать runbook:

```text
/go/proxmox-gui
/go/file1c-brief
/go/file1c-actions
```

Они формируются из `proxmox_web_gateway_routes` в nginx template и должны
возвращать `302` на целевой внутренний URL после Basic Auth.

Readiness compatibility endpoint:

```text
/portal/api/readiness -> http://127.0.0.1:8720/readyz
```

## Known Limitations

- Recovery intentionally limited to AW API poisoned lock and inactive primary
  service. Other failures remain operator-visible incidents.
- If restart does not restore AW API before timeout, secondary timers remain
  paused to avoid failure amplification. Operator must inspect
  `/var/lib/detmir-aw-primary-recovery/latest.json`.
- Windows credential or WinRM authentication problems are outside this recovery
  loop.
- This does not prove backup/restore. It is runtime availability recovery only.
