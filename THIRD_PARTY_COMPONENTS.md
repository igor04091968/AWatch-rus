# Сторонние компоненты

Основной перечень сторонних компонентов ведется в
`docs/THIRD_PARTY_LICENSES_RU.md`.

## Runtime и инфраструктура

- ActivityWatch;
- Rust crates ecosystem;
- Grafana;
- Prometheus / InfluxDB compatible metrics stack;
- Ansible;
- PowerShell / Windows Task Scheduler;
- SQLite;
- Hayabusa и связанные DFIR-инструменты при включении модуля расследования.

## Правило поставки

В публичную поставку не входят production inventory, пароли, токены, домены,
IP-адреса конкретного экземпляра, customer runtime data и локальные операторские
пути. Такие параметры задаются в приватной конфигурации экземпляра.

## Лицензии

Для собственных частей проекта используется лицензия, указанная в `LICENSE`.
Лицензии сторонних компонентов должны проверяться перед коммерческой поставкой
и фиксироваться в составе release package.
