# AWatch-rus Rust Agent Architecture

Документ описывает агентный слой AWatch-rus v0.3.

## Назначение

`awatch-agent-rs` является собственным Rust-агентом телеметрии для контура:

```text
Agent -> Telemetry -> Analytics -> Risk -> Investigation -> Report
```

Агент собирает техническую и операционную телеметрию рабочего места или сервера и отправляет ее в серверный endpoint `POST /api/telemetry`.

## Границы реализации v0.3

- Linux collector собирает реальные данные через `/proc`, `/sys`, окружение сессии и системные журналы.
- Windows collector имеет стабильный публичный интерфейс и подготовлен под WinAPI, ETW, Event Log API и WMI-библиотеки Rust.
- FreeBSD collector имеет стабильный публичный интерфейс и подготовлен под `sysctl`, `procstat`, `kvm` и стандартные интерфейсы FreeBSD.
- PowerShell не является основным механизмом сбора. Он допускается только как будущий `legacy` fallback под feature flag.
- Агент не содержит скрытых функций, драйверов ядра, кейлоггера, записи экрана, перехвата документов и контентного DLP-анализа.

## Crate

```text
adk-rust/crates/awatch-agent-rs
```

Структура:

```text
src/main.rs
src/config.rs
src/telemetry.rs
src/transport.rs
src/collectors/mod.rs
src/collectors/common.rs
src/collectors/linux.rs
src/collectors/windows.rs
src/collectors/freebsd.rs
```

## Единая модель TelemetryRecord

Все платформы должны отдавать один JSON-контракт:

- `agent_id`
- `hostname`
- `os_name`
- `os_version`
- `platform`
- `username`
- `domain`
- `timestamp`
- `uptime_seconds`
- `cpu_usage_percent`
- `memory_total`
- `memory_used`
- `active_sessions`
- `rdp_sessions`
- `ssh_sessions`
- `processes`
- `network_interfaces`
- `network_connections`
- `workforce_activity`
- `security_events`
- `collector_version`

Дополнительные структуры: `SessionInfo`, `ProcessInfo`, `NetworkInterfaceInfo`, `NetworkConnectionInfo`, `WorkforceActivityInfo`, `SecurityEventInfo`.

## Collector trait

`TelemetryCollector` задает единый контракт:

```text
collect_identity()
collect_sessions()
collect_processes()
collect_resources()
collect_network()
collect_security_events()
collect_workforce_activity()
collect_all()
```

Такой интерфейс позволяет расширять Windows, Linux, FreeBSD и pfSense mode без изменения серверного API.

## Транспорт

Транспорт использует JSON over HTTPS:

```text
POST /api/telemetry
x-api-key: <api-key>
```

При недоступности сервера запись не теряется: агент сохраняет JSON в spool и повторно отправляет после восстановления связи.
