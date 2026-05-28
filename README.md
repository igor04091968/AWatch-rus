# AWatch-rus

AWatch-rus помогает спокойно смотреть, что происходит в рабочей среде: кто работал удаленно, сколько было активного времени, какие окна были открыты, были ли события безопасности и не пропали ли данные.

Первый экран проекта теперь не про установку и скрипты. Для повседневной работы начинайте с дашбордов.

## Открыть дашборды

Основная страница:

- [Grafana dashboards](http://10.10.10.11:3000/dashboards)

Полезные панели:

- `DetMir ActivityWatch` - общая картина по активности.
- `DetMir: Работа пользователей в RDP` - кто работал, когда и в каких сессиях.
- `DetMir: DLP и ИБ обзор` - копирование, печать, USB, браузеры и другие события безопасности.
- `DetMir: ИБ сводка для руководства` - короткая управленческая сводка без лишних деталей.
- `AW-rus: DLP обзор` - отдельный обзор DLP-потока.

Дополнительные интерфейсы:

- [ActivityWatch Web UI](http://10.10.10.13:5600) - исходные события и детальный просмотр ActivityWatch.
- [Worktime reports](http://10.10.10.13:5610) - отчеты по рабочему времени, если сервис включен.

## Что видно без технических деталей

- Работал ли пользователь за компьютером или в RDP-сессии.
- Когда была активность, простой и переключение окон.
- Какие приложения, сайты и процессы чаще всего были в работе.
- Есть ли события, важные для ИБ: копирование, печать, USB, подозрительные сайты.
- Не пропали ли данные с рабочих компьютеров и RDP-сессий.

## Кому это полезно

- Руководителю - быстро увидеть рабочую картину без просмотра логов.
- ИБ - заметить DLP-сигналы и подозрительную активность.
- Администратору - проверить, что сборщики и сервер работают стабильно.

## Если дашборд пустой

Обычно это значит одно из трех: выбран слишком узкий период времени, рабочий компьютер давно не присылал события или временно не обновилась витрина в Grafana. Начните с периода `Last 24 hours`, затем переходите к техническим разделам ниже.

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
