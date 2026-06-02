# DetMir: руководство администратора

Статус: подготовительный документ для эксплуатации и пакета реестра российского
ПО.

Продуктовое имя: `DetMir`.

Техническая база и репозиторий: `AWatch-rus`.

Рекомендуемая формула для внешних документов:

> DetMir, программный комплекс на базе AWatch-rus.

## 1. Назначение администратора

Администратор отвечает за развертывание, обновление и техническое состояние
серверного и endpoint-контура DetMir.

Зона ответственности:

- сервер ActivityWatch/AW-rus;
- Rust-сервисы DetMir;
- операторский портал;
- Grafana dashboards и источники данных;
- Windows/RDP collectors;
- scheduled tasks и systemd timers;
- backup/restore;
- контроль секретов и доступов;
- проверка состояния после обновлений.

Администратор не должен использовать DetMir как сертифицированную СЗИ, DLP,
SIEM или EDR/XDR. В текущем позиционировании это платформа операционного
контроля, технического аудита и управления ИТ-инфраструктурой.

## 2. Основные компоненты

| Компонент | Назначение |
|---|---|
| AW-rus server | Прием и хранение ActivityWatch/event telemetry. |
| DetMir Rust helpers | Проверки, status, autoheal, DLP/worktime/reporting helpers. |
| DetMir Portal | Операторский web-интерфейс. |
| Evidence API | Безопасная выдача и upload подтверждений инцидентов. |
| Grafana | Витрина dashboards и управленческой аналитики. |
| Windows collectors | Сбор активности, endpoint-событий, worktime и DLP-сигналов. |
| Telegram bot | Уведомления и операторские команды; runtime остается Python. |
| Hayabusa tooling | Offline/DFIR enrichment для расследований. |

## 3. Базовые административные операции

### Проверка состояния

Минимальный набор:

```bash
detmir-status --json
detmir-check --json
detmir-dlp --json
systemctl --failed --no-pager
```

Для AW server:

```bash
aw-health-check --json
check-aw-data --json
dlp-health-check --json
```

Для Grafana:

```bash
detmir-grafana-check --json
```

### Проверка портала

Администратор проверяет:

- портал открывается через штатный gateway;
- раздел `Инциденты ИБ` показывает только релевантные DLP/incident данные;
- evidence preview/download работают только через controlled routes;
- прямые пути к файлам evidence не используются.

### Проверка evidence

Проверить:

- upload API требует Bearer token;
- upload без токена возвращает `403`;
- screenshot route отдает файл только по opaque `evidence_id`;
- SHA-256 совпадает с metadata события;
- audit пишет upload/view/download.

## 4. Управление конфигурацией

Общие правила:

1. Секреты не хранятся в git.
2. Runtime tokens не выводятся в stdout, journald, Telegram или отчеты.
3. Перед изменением systemd/drop-in/scheduled task создается backup.
4. Любой replacement legacy script на Rust проходит read-only parity и shadow
   validation.
5. Нельзя менять firewall/VPN/pfSense в рамках app-level задач без отдельного
   решения владельца.

## 5. Обновление

Стандартный порядок:

1. Проверить `git status`.
2. Прочитать relevant runbook/phase notes.
3. Собрать измененный Rust binary или применить нужный playbook.
4. Выполнить read-only smoke.
5. Переключить production unit/drop-in.
6. Проверить systemd failed units.
7. Проверить `detmir-status`.
8. Проверить портал/Grafana.
9. Записать результат в runbook.

## 6. Backup и rollback

Rollback-critical данные:

- SQLite DB ActivityWatch/AW-rus;
- DLP warehouse/cases/policy DB;
- evidence storage;
- Grafana dashboards JSON;
- Ansible inventory без публикации секретов;
- systemd unit/drop-in backups;
- Windows scheduled task/config backups.

Запрещено удалять rollback-critical backups при обычной уборке.

## 7. Инциденты

При техническом инциденте:

1. Зафиксировать текущее состояние.
2. Не запускать autoheal вслепую, если есть риск потери данных.
3. Проверить last known green state.
4. Проверить systemd/scheduled task status.
5. Проверить свежесть buckets и SLO current sample.
6. После восстановления выполнить smoke.

При DLP/ИБ-инциденте:

1. Открыть портал.
2. Проверить карточку `Инциденты ИБ`.
3. Открыть evidence metadata.
4. Сверить screenshot/SHA/audit.
5. Не удалять evidence до окончания разбора.

## 8. Связанные документы

- `docs/OPERATOR_GUIDE_RU.md`
- `docs/INSTALL_RU.md`
- `docs/ARCHITECTURE_RU.md`
- `docs/DETMIR_THREAT_MODEL_RU.md`
- `docs/DETMIR_RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md`
- `adk-rust/RUNBOOK.md`
