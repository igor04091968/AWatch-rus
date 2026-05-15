# AW-rus: презентационные экраны

Документ собирает живые экраны AW-rus и Grafana для demo, коммерческой презентации и внутренних согласований.

## 1. Работа пользователей в RDP

Управленческий экран по активности пользователей: кто работал, сколько времени, как выглядит команда по дням и кто активен сегодня.

![DetMir: Работа пользователей в RDP](assets/screenshots/grafana-rdp-worktime.png)

Источник в репозитории:

- `grafana/detmir-rdp-user-activity-dashboard.json`

## 2. DLP и ИБ обзор

Технический dashboard для ИБ: сработки, severity, типы сигналов, verdict'ы, очередь кейсов и состояние доставки данных.

![DetMir: DLP и ИБ обзор](assets/screenshots/grafana-dlp-security.png)

Источник в репозитории:

- `grafana/detmir-dlp-security-dashboard.json`

## 3. ИБ сводка для руководства

Экран для руководителя ИБ: открытые кейсы, инциденты повышенного риска, ожидающие решения события и верхнеуровневая динамика.

![DetMir: ИБ сводка для руководства](assets/screenshots/grafana-dlp-management.png)

Источник в репозитории:

- `grafana/detmir-dlp-management-dashboard.json`

## 4. AW-rus: DLP обзор

Обзорный dashboard для быстрых стендовых demo и smoke-проверки самого DLP-контура.

![AW-rus: DLP обзор](assets/screenshots/grafana-dlp-overview.png)

Источник в репозитории:

- `grafana/dlp-dashboard.json`

## 5. AW-rus summary по активности

Экран ActivityWatch-Russian для просмотра реальной активности хоста и summary по выбранному дню.

![AW-rus summary](assets/screenshots/aw-rus-summary.png)

## 6. Генерируемый отчёт по пользователям

Отдельный HTML-отчёт `RDP Worktime Report`, который система генерирует по пользователям: таблица, активное время, диапазон активности и детальные карточки по каждому сотруднику.

![RDP Worktime Report: данные по пользователям](assets/screenshots/worktime-report-users.png)

Источник в репозитории:

- `aw-server/aw-worktime-api.py`

Live endpoint:

- `http://aw-local-server:5610/reports/worktime/today?format=html&date=2026-05-15`

## 7. AW-rus: DLP review, rules и события

Экран AW-rus по bucket `aw-dlp-endpoint-signals_*`: здесь видны живые DLP-события, review-вердикты, правила, case-management и нижняя лента событий. Это прямое доказательство, что данные реально приходят на сервер до агрегации в InfluxDB/Grafana.

![AW-rus DLP bucket](assets/screenshots/aw-rus-dlp-bucket.png)

1. `RDP` dashboard — ценность для руководства и HR.
2. `DLP и ИБ обзор` — глубина технического контроля для ИБ.
3. `ИБ сводка для руководства` — понятный риск-ориентированный слой.
4. `RDP Worktime Report` — отдельный генерируемый per-user отчёт, который можно показывать как доказательство реальной работы сотрудников.
5. `AW-rus summary` и DLP review-экран — доказательство, что система не только рисует графики, а реально получает события на сервере.
