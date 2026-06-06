# AWatch-rus Platform Strategy

Документ фиксирует стратегию поддержки Windows и российских операционных
систем. Он не заявляет сертификацию, vendor-specific поддержку или готовые
дистрибутивные пакеты там, где их нет.

## Общая модель

AWatch-rus разделяет два способа сбора:

- Agent Model - установленный Rust agent/runtime на хосте.
- Agentless Model - сбор через внешние providers: PowerShell/WinRM, SSH,
  Syslog, existing logs или API.

Текущая реализация сильнее всего развита для Windows/RDP и серверного Rust
runtime. Для Linux-подобных платформ есть generic Linux collector foundation,
но отдельная поддержка конкретных российских ОС должна подтверждаться
тестированием на этих дистрибутивах.

## Windows

Current Support: `implemented`.

- Windows/RDP deployment toolkit присутствует в `windows/` и `ansible/`.
- `aw-windows-telemetry.exe` используется как Rust-first runtime для части
  Windows paths.
- Legacy PowerShell scripts остаются rollback/reference слоем до полной parity.

Agent Model: `implemented`.

- Rust Windows runtime поставляется в install-kit.
- Agent/service/task модель требует проверки на целевом host и acceptance gates.

Agentless Model: `planned`.

- PowerShell/WinRM assets существуют, но универсальный agentless provider для
  всех Windows hosts не заявляется как готовый продуктовый режим.

Future Expansion:

- расширение WinAPI/ETW/Event Log/WMI parity в Rust;
- сокращение PowerShell fallback после успешных parity gates;
- более строгая упаковка service/task definitions без runtime `.ps1`.

## Astra Linux

Current Support: `planned`.

- Отдельная Astra Linux-сертификация, пакет или distro-specific smoke в
  публичном репозитории не заявлены.
- Generic Linux collector foundation в `awatch-agent-rs` может быть базой для
  проверки совместимости.

Agent Model: `planned`.

- Целевая модель - Rust agent как systemd-friendly service с read-only probes.
- Перед коммерческим заявлением нужны установка, smoke, health check и rollback
  на конкретной версии Astra Linux.

Agentless Model: `planned`.

- Возможен SSH/Syslog/existing logs подход после реализации соответствующих
  providers.

Future Expansion:

- compatibility matrix по версиям Astra Linux;
- systemd unit/package profile;
- проверка сетевых, process и session probes без привилегий сверх необходимого.

## РЕД ОС

Current Support: `planned`.

- Отдельная поддержка РЕД ОС не подтверждена тестами в публичном репозитории.
- Generic Linux collector foundation не равен готовой поддержке РЕД ОС.

Agent Model: `planned`.

- Целевая модель - Rust agent с конфигурацией, spool/retry и systemd unit.

Agentless Model: `planned`.

- Потенциальные источники: SSH, Syslog, existing logs после появления providers.

Future Expansion:

- проверка совместимости пакетов и системных путей;
- smoke на поддерживаемых версиях РЕД ОС;
- документированный install/rollback сценарий.

## Альт

Current Support: `planned`.

- Отдельная поддержка Альт не заявлена как реализованная.
- Нужна distro-specific проверка, даже если generic Linux collector собирается
  и запускается.

Agent Model: `planned`.

- Целевая модель - Rust agent/service без зависимости от пользовательских shell
  profiles и локального состояния оператора.

Agentless Model: `planned`.

- Возможен SSH/Syslog/existing logs сценарий после реализации providers и
  нормализации событий.

Future Expansion:

- compatibility matrix по редакциям Альт;
- package/service profile;
- проверка read-only probes и прав доступа.

## РОСА

Current Support: `planned`.

- Отдельная поддержка РОСА не подтверждена готовым install profile или smoke.
- Generic Linux foundation может быть использован как техническая база.

Agent Model: `planned`.

- Целевая модель - Rust agent с systemd service, bounded timeouts и безопасным
  retry/spool.

Agentless Model: `planned`.

- Возможен после появления SSH/Syslog/API providers и проверки источников.

Future Expansion:

- smoke на выбранных версиях РОСА;
- документирование зависимостей;
- тесты свежести данных и отказоустойчивости agent/service.

## Что нельзя заявлять без отдельной реализации

- сертифицированную поддержку российских ОС;
- готовые пакеты для Astra Linux, РЕД ОС, Альт или РОСА;
- agentless-пилот без реализации providers;
- SIEM/DLP/EDR/XDR-замену;
- сбор данных из AD/LDAP/VPN/SCUD без проверенного provider и contracts.

