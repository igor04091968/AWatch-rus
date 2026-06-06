# AWatch-rus: рабочий экран

Эта страница - короткий вход в систему. Для обычной работы не нужно начинать с установки, служб и конфигов: сначала откройте дашборды и посмотрите, есть ли данные.

## Открыть дашборды

- [Grafana dashboards](http://<GRAFANA_HOST>:3000/dashboards) - основная страница со всеми панелями.
- [ActivityWatch Web UI](http://<AW_SERVER_HOST>:5600) - детальный просмотр исходных событий.
- [Worktime reports](http://<AW_SERVER_HOST>:5610) - отчеты по рабочему времени, если сервис включен.

## Что смотреть в первую очередь

- `AWatch-rus ActivityWatch` - общий обзор активности.
- `AWatch-rus: Работа пользователей в RDP` - рабочие сессии пользователей.
- `AWatch-rus: DLP и ИБ обзор` - копирование, печать, USB, браузеры и другие события безопасности.
- `AWatch-rus: ИБ сводка для руководства` - короткая картина для управленческого просмотра.
- `AW-rus: DLP обзор` - отдельный фокус на DLP-событиях.

## Скриншоты Pilot v1

Демонстрационные скриншоты от 2026-06-06:

- [Главный вывод](../screenshots/01-executive-overview.png)
- [Карта рисков подразделений](../screenshots/02-risk-heatmap.png)
- [Представление безопасности](../screenshots/03-security-view.png)
- [Представление эксплуатации](../screenshots/04-operations-view.png)
- [Пакет расследования](../screenshots/05-investigation-pack.png)
- [Итоговый отчет](../screenshots/06-markdown-report.png)
- [Архитектура продукта](../screenshots/07-product-architecture.png)

## Простая расшифровка

- Если есть активность - данные с рабочих мест приходят.
- Если графики пустые - сначала проверьте выбранный период времени.
- Если видны события безопасности - их стоит смотреть вместе с пользователем, временем и контекстом окна.
- Если данные резко пропали - переходите к разделу эксплуатации и проверок.

## Для кого

- Руководителю: посмотреть рабочую картину без логов и технической детализации.
- ИБ: увидеть события, которые могут требовать внимания.
- Администратору: быстро понять, живы ли сбор данных, API, InfluxDB и Grafana.

## Технические разделы

### Эксплуатация

- [1.2 Getting Started and Prerequisites](Getting-Started-and-Prerequisites) - обязательные переменные окружения, Influx token'ы и preflight validation.
- [2.2 Server Infrastructure](Server-Infrastructure) - сервер, retention, journald limits и `aw-prune-local-state`.
- [8 Operations, CI/CD, and Quality Assurance](Operations-CI-CD-and-Quality-Assurance) - тесты, autoheal, rollout checks и диагностика.
- [Настройка сервера](Server-Setup) - базовая настройка Linux-сервера.

### Дашборды и мониторинг

- [7 Grafana and Prometheus Monitoring Stack](Grafana-and-Prometheus-Monitoring-Stack) - Grafana, Prometheus, Influx exporters и token validation.
- [Grafana + Prometheus](Monitoring-Setup) - мониторинговый стек.
- [Prometheus Exporter](Prometheus-Exporter) - метрики для внешнего мониторинга.

### Сборщики Windows

- [3 Windows Collector Suite](Windows-Collector-Suite) - RDP/session/process collectors, recovery и локализованный Administrator.
- [Установка на Windows](Windows-Installation) - установка Windows collectors.
- [Browser Domains Monitoring](Browser-Domains-Monitoring) - сбор доменов браузеров.
- [DLP Endpoint Monitoring](DLP-Endpoint-Monitoring) - clipboard, печать, USB и endpoint-события.

### Интерфейс и отчеты

- [2.3 Russian WebUI Patch and Localization](Russian-WebUI-Patch-and-Localization) - русификация, DLP links и navigation fixes.
- [2.4 Worktime API and UI Bridge](Worktime-API-and-UI-Bridge) - API отчетов, cache, build locks и foreground context.
- [WebUI Русификация](WebUI-Russian-Patches) - патчи интерфейса.

### Архитектура и дополнительные контуры

- [Обзор архитектуры](Architecture) - высокоуровневая архитектура системы.
- [Компоненты системы](Components) - описание компонентов.
- [Интерактивная карта](Interactive-Map) - визуальная карта связей.
- [Hayabusa Security Analytics](Hayabusa-Security-Analytics) - security analytics, auto-case, scoring и Telegram alerts.
- [File 1C analytics](File-1C-Analytics) - ClickHouse/Grafana/AI Investigator контур для файловой 1С.
