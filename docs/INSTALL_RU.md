# AWatch-rus: установка и первичная проверка

Статус: подготовительный документ для пакета реестра российского ПО.

Публичное имя: `AWatch-rus`.

Техническая база и репозиторий: `AWatch-rus`.

## 1. Назначение

Документ описывает воспроизводимую установочную модель AWatch-rus без публикации
секретов и внутренних адресов production-контура.

Подробные внутренние playbooks и текущие runtime-пути описаны в
`docs/FULL_DEPLOYMENT_MANUAL_RU.md` и `adk-rust/RUNBOOK.md`. Этот документ
является публично-подготовительной версией для продукта.

## 2. Минимальные требования

Серверный контур:

- Linux server или LXC/VM;
- systemd;
- сетевой доступ от endpoint-collectors до AW-rus API;
- Rust runtime binaries из release artifacts;
- Python runtime для Telegram bot и legacy-compatible компонентов, где он еще
  используется;
- Grafana/InfluxDB/SQLite/ClickHouse по выбранному профилю установки.

Endpoint-контур:

- Windows с PowerShell 5.1+;
- права локального администратора для установки collectors;
- Scheduled Tasks;
- доступ к серверному API;
- каталог для incident artifacts/evidence sync.

Административный контур:

- Ansible;
- SSH/WinRM доступ;
- git;
- доступ к release artifacts.

## 3. Общий порядок установки

1. Подготовить сервер.
2. Установить AW-rus server.
3. Развернуть Rust binaries AWatch-rus.
4. Установить systemd units/timers.
5. Развернуть AWatch-rus Portal.
6. Настроить Grafana datasources и dashboards.
7. Настроить DLP/worktime/reporting services.
8. Развернуть Windows collectors.
9. Включить evidence upload/sync.
10. Выполнить smoke checks.

## 4. Конфигурация

Секреты задаются вне git:

- tokens;
- passwords;
- SSH/WinRM credentials;
- Grafana admin/API credentials;
- Telegram bot token;
- evidence upload token.

В git допускаются только:

- `.example` файлы;
- шаблоны переменных;
- документация;
- playbooks без секретов.

## 5. Установка серверных компонентов

Типовой путь:

```bash
cd ansible
ansible-playbook -i inventory.ini deploy_aw_server.yml
ansible-playbook -i inventory.ini deploy_detmir_portal.yml
ansible-playbook -i inventory.ini deploy_grafana_dashboards.yml
```

Для production нужно использовать актуальный inventory и group vars конкретного
контура. Секреты передаются через защищенное окружение или отдельные
непубликуемые файлы.

## 6. Установка Windows collectors

Типовой путь:

```bash
cd ansible
ansible-playbook -i inventory.ini deploy_aw_windows.yml
ansible-playbook -i inventory.ini deploy_dlp_evidence_sync.yml
```

После установки проверяются:

- scheduled tasks созданы;
- tasks выполняются без ошибок;
- ActivityWatch buckets получают события;
- DLP/evidence sync task возвращает успешный статус.

## 7. Первичная проверка

Минимальный smoke:

```bash
detmir-status --json
detmir-check --json
detmir-dlp --json
detmir-grafana-check --json
```

Ожидается:

- status OK;
- failed systemd units отсутствуют;
- AW API отвечает;
- Grafana datasources healthy;
- mandatory dashboards не stale;
- portal health OK.

## 8. Проверка evidence workflow

Контролируемый тест:

1. Создать тестовый incident artifact на endpoint.
2. Дождаться scheduled sync или запустить его вручную.
3. Проверить upload на сервере.
4. Проверить portal evidence metadata.
5. Открыть preview/download.
6. Проверить audit.
7. Удалить тестовый incident/artifact/state.

Тестовые artifacts должны быть явно помечены как synthetic/smoke.

## 9. Завершение установки

Перед передачей в эксплуатацию:

- сохранить версии release artifacts;
- сохранить контрольный `detmir-status`;
- сохранить список active services/timers;
- сохранить список Grafana dashboards;
- проверить backup/restore;
- проверить, что ноутбук администратора не является runtime-зависимостью.

## 10. Связанные документы

- `docs/ADMIN_GUIDE_RU.md`
- `docs/ARCHITECTURE_RU.md`
- `docs/FULL_DEPLOYMENT_MANUAL_RU.md`
- `docs/GRAFANA_DASHBOARDS_RU.md`
- `adk-rust/RUNBOOK.md`
