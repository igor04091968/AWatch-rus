# pfSense Integration

pfSense рассматривается как интеграционный слой сетевого периметра, а не обязательная часть продукта.

## Режим v0.3

- read-only;
- без изменения правил;
- без автоматического карантина;
- без управления маршрутизацией;
- без зависимости портала от pfSense.

## Место в архитектуре

```text
pfSense / firewall telemetry
        |
awatch-agent-rs --role firewall
        |
POST /api/telemetry
        |
Workforce/UEBA risk context
```

## Коммерческая ценность

Интеграция позволяет объяснять риски не только по активности рабочего места, но и по сетевому контексту: unusual destinations, VPN sessions, gateway status, proxy/DNS signals.

## Не реализуется в v0.3

- NAC;
- SOAR-автоматизация;
- блокировка VLAN;
- изменение firewall rules;
- управление VPN-доступом.
