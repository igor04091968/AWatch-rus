# AWatch-rus BSD Support

## Цель

BSD-слой нужен для будущей поддержки FreeBSD и pfSense mode без изменения публичного `TelemetryRecord`.

## FreeBSD collector

В v0.3 FreeBSD collector имеет стабильный интерфейс и честно возвращает ограничение реализации, если бинарник собран под FreeBSD до включения нативных источников.

Планируемые источники:

- `sysctl`;
- `procstat`;
- `kvm`;
- сетевые интерфейсы ОС;
- системный syslog;
- сведения об SSH-сессиях.

## pfSense mode

Роль:

```bash
awatch-agent-rs --role firewall
```

Назначение: read-only сбор телеметрии сетевого периметра.

Будущие сигналы:

- interfaces;
- gateway status;
- VPN sessions;
- firewall counters;
- pf statistics;
- NAT counters;
- DNS statistics;
- Suricata summary, если установлен;
- Unbound summary, если установлен.

## Ограничение безопасности

pfSense mode не должен менять правила firewall, NAT, DNS, VPN или маршрутизацию. В рамках v0.3 это только наблюдение.
