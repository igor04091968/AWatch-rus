# Сведения для подачи в реестр российского ПО

Статус документа: рабочий пакет для подготовки продукта `AWatch-rus` к
экспертной проверке и возможной подаче в реестр российского ПО.

Документ намеренно описывает продукт как программный комплекс операционного
контроля, технического аудита и управления ИТ-инфраструктурой. Продукт не
заявляется как сертифицированная DLP, SIEM, EDR/XDR или средство защиты
информации.

## 1. Наименование продукта

Публичное наименование:

- `AWatch-rus`.

Техническая база и репозиторий:

- `AWatch-rus`.

Рекомендуемая формула для документов:

```text
Программный продукт AWatch-rus.
```

Для публичных материалов использовать единую формулу: `Программный продукт
AWatch-rus`. Это не создает второго бренда и не отделяет продукт от
репозитория.

## 2. Назначение ПО

`AWatch-rus` предназначен для централизованного операционного контроля,
технического аудита и мониторинга ИТ-инфраструктуры организации.

Основные задачи:

- контроль состояния серверных сервисов, endpoint-сборщиков и витрин данных;
- учет пользовательской активности, рабочих интервалов и удаленных сессий;
- мониторинг свежести данных ActivityWatch и связанных buckets;
- контроль выполнения эксплуатационных регламентов и runbook-проверок;
- SLO/health мониторинг и безопасная автоматизация восстановления;
- отображение управленческих и технических dashboards;
- фиксация evidence по прикладным инцидентам;
- аудит действий оператора и техническая трассировка расследований.

Продукт закрывает задачу эксплуатационной видимости: администратор,
оператор ИБ или руководитель видит, что сбор данных идет, инфраструктурные
компоненты доступны, данные обновляются, а прикладные инциденты имеют
прослеживаемую evidence-цепочку.

## 3. Класс ПО

Основной целевой класс для реестра:

```text
09.10 Средства управления ИТ-службой, ИТ-инфраструктурой и ИТ-активами
```

Обоснование:

- продукт контролирует состояние ИТ-сервисов и инфраструктурных компонентов;
- содержит operational dashboards, health-check и SLO-мониторинг;
- автоматизирует эксплуатационные проверки и безопасные recovery-действия;
- хранит технические состояния, отчеты, evidence и audit trail;
- применяется для контроля работоспособности и наблюдаемости корпоративного
  контура.

Дополнительный контекст, который можно использовать в описании:

- технический аудит;
- интеллектуальный мониторинг инфраструктуры;
- автоматизация runbook-процессов;
- контроль регламентов эксплуатации.

Не рекомендуется заявлять продукт как:

- сертифицированную DLP;
- SIEM;
- EDR/XDR;
- средство защиты информации;
- продукт с формальной ФСТЭК-моделью угроз.

Модули DLP/evidence/Hayabusa описываются как прикладные модули операционного
контроля и расследования событий, а не как самостоятельная сертифицированная
система защиты информации.

## 4. Правообладатель

Правообладатель: владелец репозитория и программного продукта `AWatch-rus`.

Перед подачей в реестр рекомендуется подготовить отдельный
правообладательский пакет:

- сведения о правообладателе;
- описание прав на собственные модули;
- подтверждение авторства или передачи прав на разработанные компоненты;
- перечень сторонних компонентов и лицензий;
- описание модели распространения;
- при необходимости - свидетельство Роспатента о регистрации программы для ЭВМ.

Собственными компонентами считаются:

- Rust helpers и runtime-модули AWatch-rus;
- портал оператора;
- Ansible deployment automation;
- Windows collectors/deployment scripts;
- ActivityWatch RU customization;
- Grafana dashboards проекта;
- документация, runbooks и install-kit packaging.

Сторонние компоненты перечислены отдельно в `THIRD_PARTY_LICENSES_RU.md` и
`docs/THIRD_PARTY_LICENSES_RU.md`.

## 5. Состав поставки

Публичная поставка состоит из исходного кода, документации и шаблонов
конфигурации. Индивидуальные параметры конкретного стенда не входят в
публичную поставку.

В состав входят:

- `adk-rust/` - Rust workspace с основными runtime helpers;
- `ansible/` - playbooks и examples для установки серверных и endpoint
  компонентов;
- `aw-server/` - ActivityWatch server customization, service files,
  RU WebUI patches и server-side helpers;
- `windows/` - Windows collectors, scheduled task deployment и common module;
- `grafana/` - dashboards для технического и управленческого мониторинга;
- `proxmox/` - операторские helpers, включая Telegram runtime, если он
  используется в конкретном экземпляре;
- `docs/` - руководства администратора, оператора, архитектура, threat model,
  registry positioning и runbooks;
- `private-config/*.example` - шаблоны приватной конфигурации;
- release assets - install-kit archives для проверяемых сборок.

Не входят в публичный репозиторий:

- production inventory;
- пароли;
- токены;
- реальные IP-адреса и домены экземпляра;
- runtime базы данных и evidence;
- customer deployment snapshots;
- локальная история работы операторских ИИ-агентов.

## 6. Функциональный состав

### 6.1. Контроль ActivityWatch telemetry

- проверка доступности AW API;
- контроль свежести buckets;
- учет event-driven buckets без ложного dead/stale статуса;
- health summary для оператора;
- SLO sampling и summary.

### 6.2. Учет активности и рабочего времени

- обработка window/AFK/session данных;
- отчеты по активному времени;
- поддержка RDP/Windows collector flow;
- InfluxDB/Grafana витрины;
- heartbeat freshness для контроля работы exporter-а.

### 6.3. Операционный контроль и auto-heal

- `detmir-check`;
- `detmir-status`;
- `detmir-auto`;
- безопасные recovery paths;
- контроль systemd timers/services;
- исключение опасных destructive actions из автоматического режима.

### 6.4. Evidence и расследования

- хранение evidence metadata;
- screenshot/evidence viewer в портале оператора;
- audit записи просмотра evidence;
- Hayabusa/offline DFIR flow как прикладной модуль расследования.

### 6.5. Визуализация

- Grafana dashboards;
- портал оператора;
- management views для руководителя;
- technical views для администратора и оператора ИБ.

## 7. Архитектура

Типовая архитектура экземпляра:

```text
Windows/Linux endpoints
        |
        v
ActivityWatch collectors / endpoint helpers
        |
        v
AW server + AWatch-rus Rust helpers
        |
        +--> SQLite state/cases/policy/evidence metadata
        +--> InfluxDB/metrics storage
        +--> Grafana dashboards
        +--> AWatch-rus operator portal
        +--> Telegram/operator runtime, если включен
```

Ядро AWatch-rus реализовано как Rust-first runtime:

- health/status/check helpers;
- worktime exporters/API/bridges;
- DLP server-side processing helpers;
- evidence API/portal helpers;
- install-kit validation tools;
- operational quality gates.

Python в составе проекта не является основным ядром продукта. Он остается для:

- Telegram bot runtime, если используется заказчиком;
- OCR/content-analysis path, где нужны Python OCR/ML библиотеки;
- 1C/AI/ETL интеграций;
- отдельных MCP/dev helper сценариев.

Такое разделение фиксируется как архитектурное: критичные серверные проверки,
status path, SLO, worktime, DLP server-side helpers и install-kit tooling
переведены на Rust-first модель.

## 8. Зависимости

Основные runtime dependencies:

- Linux/systemd;
- ActivityWatch;
- Rust runtime artifacts, собранные из `adk-rust`;
- SQLite;
- Grafana;
- InfluxDB или совместимое хранилище временных рядов, если включены metrics;
- Ansible для установки;
- PowerShell/Windows Task Scheduler для Windows collectors;
- Hayabusa для offline DFIR workflow, если включен;
- Python только для согласованных вспомогательных модулей.

Сторонние лицензии и риски AGPL/GPL/weak copyleft описаны в
`THIRD_PARTY_LICENSES_RU.md`.

## 9. Установка экземпляра

Короткий порядок для эксперта:

1. Склонировать репозиторий.
2. Подготовить приватную конфигурацию:

   ```bash
   cp private-config/deploy.env.example private-config/deploy.env
   cp ansible/inventory.example.ini ansible/inventory.ini
   ```

3. Заполнить параметры конкретного тестового экземпляра.
4. Собрать Rust artifacts:

   ```bash
   cd adk-rust
   cargo build --release --workspace
   ```

5. Выполнить syntax и quality checks:

   ```bash
   scripts/quality-gate.sh
   ansible-playbook --syntax-check -i ansible/inventory.ini ansible/deploy_aw_server.yml
   ```

6. Установить серверные компоненты и collectors по `docs/INSTALL_RU.md`.
7. Проверить работоспособность:

   ```bash
   detmir-check
   detmir-status
   ```

Ожидаемый результат: статус `OK`, отсутствуют критичные service failures и
stale/dead buckets для обязательных источников.

## 10. Ограничения

- Продукт не заменяет формально сертифицированные средства защиты информации
  без отдельной сертификации.
- Реальные сетевые адреса, домены, токены и inventory являются параметрами
  экземпляра и не публикуются.
- Для endpoint deployment нужны административные права.
- Для некоторых прикладных модулей нужны внешние сервисы: Grafana, InfluxDB,
  Hayabusa или Python OCR stack.
- License compatibility сторонних компонентов должна проверяться перед
  коммерческой поставкой.

## 11. Документы пакета

- `PRODUCT_DESCRIPTION_RU.md` - краткое описание продукта.
- `CHANGELOG_RU.md` - журнал изменений и статус публичных release-пакетов.
- `INSTALL_FOR_EXPERT_RU.md` - короткая инструкция установки экземпляра.
- `docs/EXPERT_TEST_SCENARIO_RU.md` - ручной сценарий экспертной проверки после установки.
- `docs/RELEASE_MANIFEST_2026-06.md` - manifest release artifacts, checksums и gates.
- `THIRD_PARTY_COMPONENTS.md` - обзор сторонних компонентов.
- `THIRD_PARTY_LICENSES_RU.md` - лицензии и license-audit checklist.
- `docs/ARCHITECTURE_RU.md` - архитектура.
- `docs/ADMIN_GUIDE_RU.md` - руководство администратора.
- `docs/OPERATOR_GUIDE_RU.md` - руководство оператора.
- `docs/RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md` - стратегия
  позиционирования.
