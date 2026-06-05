# AWatch-rus

AWatch-rus / DetMir - программный комплекс операционного контроля,
технического аудита, оценки трудоотдачи сотрудников и мониторинга
корпоративной ИТ-инфраструктуры на базе ActivityWatch, Rust-сервисов
автоматизации, Grafana/Prometheus-витрин и модулей расследования инцидентов.

Проект не позиционируется как сертифицированная DLP/SIEM/EDR/XDR/СЗИ. DLP,
evidence и Hayabusa используются как прикладные модули внутри платформы
операционного контроля и технического аудита.

## Назначение

- DetMir Workforce: активность сотрудников, загрузка, RDP/1C/рабочие
  приложения и управленческие отчеты для владельца бизнеса.
- DetMir Security: DLP-сигналы, evidence, очередь кейсов и audit действий
  оператора без заявления продукта как сертифицированной СЗИ.
- DetMir Forensics: цепочки событий, Hayabusa/offline-разбор и материалы для
  внутреннего расследования.
- Контроль доступности и свежести данных ActivityWatch.
- Учет активного времени, RDP-сессий, окон, приложений и рабочих интервалов.
- Витрины Grafana для администратора, оператора ИБ и руководителя.
- Автоматизация runbook-проверок, health-check, SLO и безопасного auto-heal.
- Сбор evidence по инцидентам и аудит действий оператора.

## Rust-first runtime

Основной серверный runtime DetMir переведен на Rust: status/check/auto-heal,
SLO, worktime, DLP server-side helpers, evidence и install-kit tooling.

Python в репозитории остается для вспомогательных направлений: Telegram bot
runtime, OCR/content-analysis, 1C/AI/ETL integration и MCP/dev helpers. Эти
части не являются ядром Rust-first runtime.

## Что видит оператор

- Работал ли пользователь за компьютером или в удаленной сессии.
- Когда была активность, простой и переключение окон.
- Какие приложения, сайты и процессы чаще всего были в работе.
- Есть ли события, важные для ИБ: копирование, печать, USB, подозрительные сайты.
- Не пропали ли данные с рабочих компьютеров и RDP-сессий.

## Кому это полезно

- Владельцу и руководителю - видеть активность, загрузку команды,
  простои, перегрузки и рабочие приложения без просмотра логов.
- ИБ - заметить DLP-сигналы и подозрительную активность.
- Администратору - проверить, что сборщики и сервер работают стабильно.

## Если дашборд пустой

Обычно это значит одно из трех: выбран слишком узкий период времени, рабочий компьютер давно не присылал события или временно не обновилась витрина в Grafana. Начните с периода `Last 24 hours`, затем переходите к техническим разделам ниже.

## Поставка и регистрация

- [Позиционирование для реестра российского ПО](docs/DETMIR_RUSSIAN_SOFTWARE_REGISTRY_POSITIONING_RU.md)
- [Сведения для подачи в реестр](REGISTER_RU_SOFTWARE.md)
- [Описание продукта](PRODUCT_DESCRIPTION_RU.md)
- [Журнал изменений](CHANGELOG_RU.md)
- [Установка для эксперта](INSTALL_FOR_EXPERT_RU.md)
- [Сценарий экспертной проверки](docs/EXPERT_TEST_SCENARIO_RU.md)
- [Release manifest 2026-06](docs/RELEASE_MANIFEST_2026-06.md)
- [Эксплуатационный профиль](docs/OPERATIONAL_PROOF_PROFILE_RU.md)
- [Коммерческие модули DetMir](docs/DETMIR_COMMERCIAL_MODULES_RU.md)
- [Архитектурный baseline](docs/ARCHITECTURE_BASELINE_RU.md)
- [Сторонние компоненты](THIRD_PARTY_COMPONENTS.md)
- [Сторонние лицензии](THIRD_PARTY_LICENSES_RU.md)
- [Архитектура](docs/ARCHITECTURE_RU.md)
- [Установка](docs/INSTALL_RU.md)
- [Руководство администратора](docs/ADMIN_GUIDE_RU.md)
- [Руководство оператора](docs/OPERATOR_GUIDE_RU.md)
- [Лицензия](LICENSE)

## Техническая документация

Для эксплуатации и настройки:

- [Wiki home](docs/wiki/Home.md)
- [Getting Started and Prerequisites](docs/wiki/Getting-Started-and-Prerequisites.md)
- [Server Infrastructure](docs/wiki/Server-Infrastructure.md)
- [Operations, CI/CD, and Quality Assurance](docs/wiki/Operations-CI-CD-and-Quality-Assurance.md)
- [Full deployment manual](docs/FULL_DEPLOYMENT_MANUAL_RU.md)

Для мониторинга:

- [Grafana and Prometheus Monitoring Stack](docs/wiki/Grafana-and-Prometheus-Monitoring-Stack.md)
- [Grafana dashboards guide](docs/GRAFANA_DASHBOARDS_RU.md)
- [Prometheus Exporter](docs/wiki/Prometheus-Exporter.md)

Для сборщиков и интерфейса:

- [Windows Collector Suite](docs/wiki/Windows-Collector-Suite.md)
- [Worktime API and UI Bridge](docs/wiki/Worktime-API-and-UI-Bridge.md)
- [Russian WebUI Patch and Localization](docs/wiki/Russian-WebUI-Patch-and-Localization.md)
