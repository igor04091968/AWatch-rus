# AWatch-rus Pilot v1.0

## Позиционирование

AWatch-rus Pilot v1.0 показывается заказчику как связка:

- Workforce Analytics - активность, загрузка, сравнение подразделений и отчетность.
- Security Analytics - кандидаты на проверку, события безопасности и объяснимый риск.
- Forensics - карточки расследований, timeline и выгрузка материалов.

Это не SIEM, не классический DLP, не EDR/XDR и не сертифицированная СЗИ.
Сетевой периметр pfSense является опциональным интеграционным слоем, а не
обязательной частью пилота.

Основной интерфейс Pilot v1.0: Rust server-rendered HTML + JSON API. Dioxus не
используется и не рассматривается для этого контура. React, Tauri и Electron
также не входят в текущий пилотный UI.

## Готовые контуры

- Executive Dashboard: главный вывод, общий статус, риски подразделений и
  краткий управленческий срез.
- Workforce Portal: индекс активности, подразделения, ответственные, тренды,
  признаки перегруза и недогруза, Markdown-отчет.
- Security Portal: кандидаты на проверку, risk score, события безопасности,
  аномалии и аудит ручных решений.
- Forensics Portal: карточки расследований, timeline, пакет материалов и
  Markdown export.
- UEBA Score v1: прозрачная rule-based модель без ML, LLM и внешних SaaS.
- pfSense readiness: contracts, demo fixtures, docs и API-заготовка без
  заявления о включенном ingestion.

## API v1

Ролевые endpoint-ы:

- `/api/executive`
- `/api/workforce`
- `/api/security`
- `/api/forensics`
- `/api/ueba`
- `/api/pfsense`
- `/api/reports`

Все endpoint-ы возвращают стабильные JSON-структуры. Клиент должен игнорировать
неизвестные поля и не должен трактовать отсутствие опционального поля как ошибку.

## Роли

Роли описаны в [ROLES_RU.md](ROLES_RU.md). Проверка доступа выполняется на
сервере, а не только скрытием кнопок в HTML.

## Demo data

Демонстрационные данные не должны содержать реальные IP-адреса, hostname,
логины, ФИО, подразделения заказчика или реальные события безопасности. Для
сетевых примеров используются адреса из RFC 5737: `192.0.2.0/24`,
`198.51.100.0/24`, `203.0.113.0/24`.

## Проверки перед показом

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --workspace --release`
- `git diff --check`
- `node scripts/detmir-portal-tabs-smoke.mjs`
