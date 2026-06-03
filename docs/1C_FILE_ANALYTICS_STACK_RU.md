# Файловая 1С Detmir: промышленное развёртывание ClickHouse/Grafana контура

Документ фиксирует **production-схему** для файловой 1С без вмешательства в содержимое базы.

Контур предназначен для среды, где:

- 1С работает как **файловая база** на Windows/RDP host;
- на хост 1С нельзя ставить тяжёлые сервисы;
- нужен audit/detection/investigation слой, а не только KPI;
- Grafana уже поднята отдельно от ноутбука.

Документ описывает **реально проверенную** схему, а не только scaffold.

## 1. Границы и гарантии

Этот контур:

- **не** открывает 1С через `COM`, `Configurator`, `Designer`;
- **не** меняет `1Cv8.1CD`;
- работает только как `read-only export/telemetry` вокруг файловой базы;
- читает:
  - `ibases.v8i`,
  - наличие и размеры `1Cv8.1CD`,
  - `1Cv8Log`,
  - файловые маркеры занятости,
  - host telemetry Windows.

Это принципиально. Любые действия, которые пишут обратно в 1С, в этот контур не входят.

## 2. Production topology

### 2.1 Узлы

- `<WINDOWS_HOST>`
  - Windows / RDP host с файловой 1С
  - источник `read-only` telemetry/export
- `<GATEWAY_HOST>`
  - backend узел file-1C analytics
  - `ClickHouse`
  - ETL/ingest
  - detections
  - cases
  - proof-check
- `<GRAFANA_HOST>`
  - production `Grafana`
  - готовые dashboards
- `<AW_SERVER_HOST>`
  - основной `AW-rus` сервер
  - в file-1C pipeline не является обязательным runtime-компонентом

### 2.2 Поток данных

```text
Windows file 1C host (<WINDOWS_HOST>)
  ├─ ibases.v8i inventory
  ├─ 1Cv8.1CD file metadata
  ├─ 1Cv8Log metadata
  ├─ file-base busy markers
  └─ host telemetry
          ↓
export-upload-file-1c-telemetry.ps1
          ↓  scp
<GATEWAY_HOST> /opt/activitywatch/clickhouse-1c/landing/*
          ↓
run_ingest_cycle.sh
  ├─ raw tables
  ├─ core tables
  ├─ entity_timeline
  ├─ detections
  └─ cases
          ↓
Grafana <GRAFANA_HOST>
```

## 3. Что считается готовым контуром

Контур считается рабочим, если одновременно выполняется всё:

1. Windows scheduled task `ActivityWatch File1C Upload` запускается по расписанию.
2. На `<GATEWAY_HOST>` работает `aw-1c-ingest.timer`.
3. На `<GATEWAY_HOST>` работает `aw-1c-proofcheck.timer`.
4. `ClickHouse` содержит живые строки в:
   - `documents`
   - `reglog_events`
   - `audit_events`
   - `host_events`
   - `entity_timeline`
   - `detections`
   - `cases`
5. В `Grafana` на `<GRAFANA_HOST>` dashboards открываются и смотрят в datasource `clickhouse-1c`.

## 4. Каталоги и артефакты

### 4.1 На Windows `<WINDOWS_HOST>`

- `C:\ProgramData\AWatch-rus\deployment-config.json`
- `C:\ProgramData\AWatch-rus\export-upload-file-1c-telemetry.ps1`
- `C:\ProgramData\AWatch-rus\logs\file1c-telemetry.log`
- `C:\ProgramData\AWatch-rus\ssh\awops_ed25519`

### 4.2 На backend `<GATEWAY_HOST>`

- root:
  - `/opt/activitywatch/clickhouse-1c`
- landing:
  - `/opt/activitywatch/clickhouse-1c/landing/documents`
  - `/opt/activitywatch/clickhouse-1c/landing/reglog`
  - `/opt/activitywatch/clickhouse-1c/landing/audit`
  - `/opt/activitywatch/clickhouse-1c/landing/host`
- archive:
  - `/opt/activitywatch/clickhouse-1c/archive`
- runtime:
  - `/opt/activitywatch/clickhouse-1c/.env`
  - `/opt/activitywatch/clickhouse-1c/etl/config.yml`
  - `/opt/activitywatch/clickhouse-1c/.venv`

### 4.3 systemd units на `<GATEWAY_HOST>`

- `aw-1c-ingest.service`
- `aw-1c-ingest.timer`
- `aw-1c-proofcheck.service`
- `aw-1c-proofcheck.timer`

## 5. Развёртывание с нуля

### 5.1 Backend на `<GATEWAY_HOST>`

Playbook:

- [ansible/deploy_file_1c_analytics.yml](<PROJECT_ROOT>/ansible/deploy_file_1c_analytics.yml)

Команда:

```bash
ansible-playbook -i <PROJECT_ROOT>/ansible/inventory.ini \
  <PROJECT_ROOT>/ansible/deploy_file_1c_analytics.yml
```

Что делает:

- ставит `docker.io`, `docker-compose`, `python3-venv`, `python3-pip`;
- раскладывает `clickhouse-1c` в `/opt/activitywatch/clickhouse-1c`;
- поднимает `ClickHouse`;
- создаёт `.env` и `etl/config.yml`;
- включает `aw-1c-ingest.timer`;
- включает `aw-1c-proofcheck.timer`.

### 5.2 Windows uploader на `<WINDOWS_HOST>`

Playbook:

- [ansible/deploy_file_1c_windows_telemetry.yml](<PROJECT_ROOT>/ansible/deploy_file_1c_windows_telemetry.yml)

Команда:

```bash
ansible-playbook -i <PROJECT_ROOT>/ansible/inventory.ini \
  <PROJECT_ROOT>/ansible/deploy_file_1c_windows_telemetry.yml
```

Что делает:

- копирует `export-upload-file-1c-telemetry.ps1`;
- обновляет `deployment-config.json`;
- создаёт/обновляет scheduled task `ActivityWatch File1C Upload`.

### 5.3 Production Grafana на `<GRAFANA_HOST>`

Grafana уже должна содержать:

- datasource `clickhouse-1c`
- folder `1C File Analytics`
- dashboards:
  - `1c-file-exec`
  - `1c-file-ops`
  - `1c-file-audit`
  - `1c-file-detections`
  - `1c-file-investigation`
  - `1c-file-data-quality`

См.:

- [docs/1C_GRAFANA_DEPLOYMENT_RU.md](<PROJECT_ROOT>/docs/1C_GRAFANA_DEPLOYMENT_RU.md)

## 6. Обязательный post-step на Windows

### 6.1 Почему он нужен

Создание file-1C scheduled task через `schtasks` по умолчанию использует `SYSTEM`.

Для этой конкретной задачи production-схема должна использовать **рабочий principal**, а не `SYSTEM`.

Проверенная рабочая учётка:

- `HOST-EXAMPLE\Администратор`

### 6.2 Команда переключения principal

На `<WINDOWS_HOST>`:

```cmd
schtasks /Change /TN "\ActivityWatch File1C Upload" /RU "HOST-EXAMPLE\Администратор" /RP "<LOCAL_ADMIN_PASSWORD>"
```

### 6.3 Проверка

```cmd
schtasks /Query /TN "\ActivityWatch File1C Upload" /V /FO LIST
```

Ожидается:

- `Run As User: Администратор`
- `Last Result: 0`

### 6.4 Важный нюанс

Код rollout уже пропатчен так, чтобы **не откатывать существующий principal обратно на `SYSTEM`** при следующих targeted deploy.

Это защита от регрессии, но не хранение пароля в репозитории.

## 7. Ручная верификация после deploy

### 7.1 Windows task

```cmd
schtasks /Run /TN "\ActivityWatch File1C Upload"
schtasks /Query /TN "\ActivityWatch File1C Upload" /V /FO LIST
```

Локальный лог:

```powershell
Get-Content -Tail 80 C:\ProgramData\AWatch-rus\logs\file1c-telemetry.log
```

### 7.2 Backend ingestion

```bash
AW_1C_ROOT=/opt/activitywatch/clickhouse-1c /opt/activitywatch/clickhouse-1c/ops/run_ingest_cycle.sh
```

### 7.3 Freshness proof

```bash
AW_1C_ROOT=/opt/activitywatch/clickhouse-1c /opt/activitywatch/clickhouse-1c/ops/check_ingest_freshness.sh
```

Ожидается строка вида:

```text
freshness documents=0h reglog=0h audit=0h host=0h threshold=8h
```

### 7.4 Счётчики ClickHouse

```bash
docker exec aw-rus-1c-clickhouse clickhouse-client \
  --user default --password change-me --database analytics_1c \
  -q "SELECT count() FROM documents"
```

Аналогично:

- `reglog_events`
- `audit_events`
- `host_events`
- `entity_timeline`
- `detections`
- `cases`

## 8. Production state, подтверждённое в этой среде

Подтверждённые живые значения:

- `documents = 46`
- `reglog_events = 94`
- `audit_events = 46`
- `host_events = 1`
- `entity_timeline = 186`
- `detections = 12`
- `cases = 12`

Подтверждённые состояния:

- `aw-1c-proofcheck.timer = active/enabled`
- scheduled task `\ActivityWatch File1C Upload`:
  - `Run As User: Администратор`
  - `Last Result: 0`

## 9. Что именно собирается

### 9.1 `documents`

Не документы изнутри 1С, а inventory snapshot по файловым базам:

- имя infobase;
- owner;
- статус `online/busy`;
- стабильно вычисляемый `doc_id` по `baseId` или path.

### 9.2 `reglog_events`

Не парсинг бинарного reglog, а metadata/operational signals:

- наличие и активность `1Cv8Log`;
- размер `.lgp`;
- busy markers;
- `1Cv8JobScheduler`.

### 9.3 `audit_events`

Snapshot-события по infobase:

- `inventory_snapshot`
- `risk_tag=busy`, если база занята.

### 9.4 `host_events`

Минимальный безопасный host telemetry слой:

- CPU
- RAM
- free disk
- RDP sessions
- backup flag

## 10. Hardening, уже внесённый в контур

В production-контуре уже реализовано:

- `scp` retries на Windows;
- абсолютный путь до `scp.exe`;
- runtime logging exporter-а;
- BOM-safe loader;
- `flock`-lock на ingest cycle;
- `min_file_age_seconds=180` против чтения недокачанных файлов;
- `proofcheck.timer` каждые 6 часов;
- auto-case по detections;
- защита от отката task principal при targeted deploy.

## 11. Известные failure modes

### 11.1 `Last Result != 0` у scheduled task

Проверить:

```powershell
Get-Content -Tail 100 C:\ProgramData\AWatch-rus\logs\file1c-telemetry.log
```

Типовые причины:

- wrong principal;
- нет доступа к `awops_ed25519`;
- runtime ошибка PowerShell;
- не найден `scp.exe`.

### 11.2 `proofcheck` красный

Проверить:

```bash
systemctl status aw-1c-proofcheck.timer
systemctl status aw-1c-ingest.timer
AW_1C_ROOT=/opt/activitywatch/clickhouse-1c /opt/activitywatch/clickhouse-1c/ops/check_ingest_freshness.sh
```

Типовые причины:

- Windows task не сработал;
- файлы не попали в `landing`;
- ingest не сработал;
- `ClickHouse` не поднят.

### 11.3 `ClickHouse` пустой

Проверить:

```bash
ls -la /opt/activitywatch/clickhouse-1c/landing/documents
ls -la /opt/activitywatch/clickhouse-1c/archive/documents
docker ps --format '{{.Names}}' | rg aw-rus-1c-clickhouse
```

## 12. Recovery-порядок

### 12.1 Windows сторона

1. Проверить scheduled task.
2. Проверить лог `file1c-telemetry.log`.
3. Запустить exporter вручную:

```powershell
& "C:\ProgramData\AWatch-rus\export-upload-file-1c-telemetry.ps1" -ConfigPath "C:\ProgramData\AWatch-rus\deployment-config.json"
```

### 12.2 Backend сторона

1. Проверить `ClickHouse`.
2. Проверить `landing`.
3. Запустить ingest вручную:

```bash
AW_1C_ROOT=/opt/activitywatch/clickhouse-1c /opt/activitywatch/clickhouse-1c/ops/run_ingest_cycle.sh
```

4. Проверить freshness.

## 13. Безопасность

- Не хранить пароль администратора Windows в git.
- Не хранить секреты в docs.
- Не переводить этот контур в `COM`/`Designer` без отдельного решения.
- Не давать AI write-back в 1С.
- Не менять `1Cv8.1CD`.

## 14. Связанные файлы

- [clickhouse-1c/README.md](<PROJECT_ROOT>/clickhouse-1c/README.md)
- [clickhouse-1c/etl/load_1c_exports.py](<PROJECT_ROOT>/clickhouse-1c/etl/load_1c_exports.py)
- [clickhouse-1c/ops/run_ingest_cycle.sh](<PROJECT_ROOT>/clickhouse-1c/ops/run_ingest_cycle.sh)
- [clickhouse-1c/ops/check_ingest_freshness.sh](<PROJECT_ROOT>/clickhouse-1c/ops/check_ingest_freshness.sh)
- [ansible/deploy_file_1c_analytics.yml](<PROJECT_ROOT>/ansible/deploy_file_1c_analytics.yml)
- [ansible/deploy_file_1c_windows_telemetry.yml](<PROJECT_ROOT>/ansible/deploy_file_1c_windows_telemetry.yml)
- [windows/export-upload-file-1c-telemetry.ps1](<PROJECT_ROOT>/windows/export-upload-file-1c-telemetry.ps1)
- [docs/wiki/File-1C-Analytics.md](<PROJECT_ROOT>/docs/wiki/File-1C-Analytics.md)
