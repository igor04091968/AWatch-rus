# Security Hardening

Документ описывает базовые меры hardening для AWatch-rus deployment.

Это не сертификационная модель угроз и не заявление о готовности как СЗИ.

## TLS

Рекомендуется:

- публиковать portal только через TLS;
- использовать certificate lifecycle process;
- отключать устаревшие протоколы;
- проверять expiry до demo/production use;
- хранить private keys вне repository.

## Reverse Proxy

Reverse proxy должен:

- завершать TLS;
- ограничивать public exposure;
- передавать только необходимые routes;
- добавлять security headers where policy allows;
- вести access logs без secrets.

## Firewall

Рекомендуется:

- открыть только необходимые ports;
- ограничить admin endpoints;
- запретить прямой доступ к backend, если используется reverse proxy;
- фиксировать правила в deployment documentation;
- проверять правила после изменений.

## Учетные записи

Рекомендуется:

- отдельные service accounts;
- минимально необходимые права;
- запрет shared admin credentials;
- регулярная ротация secrets;
- хранение secrets в защищенном хранилище заказчика.

## Права доступа

Проверять:

- role-based portal access;
- server-side role gates;
- access to evidence materials;
- admin-only operations;
- file permissions for config/state/evidence metadata.

## Журналирование

Логи должны помогать диагностике, но не раскрывать:

- passwords;
- tokens;
- private keys;
- customer evidence;
- персональные данные без необходимости.

## Backup Security

Backup storage должен:

- быть доступен только ответственным ролям;
- хранить encrypted backups where required;
- иметь restore test;
- не публиковаться в Git.

## Demo and Public Materials

Для публичных материалов:

- использовать только demo fixtures;
- использовать TEST-NET addresses for network examples;
- не публиковать live screenshots;
- не публиковать customer hostnames, users, domains or evidence.

## Ограничения

AWatch-rus hardening guide не заявляет:

- сертифицированную защиту информации;
- полноценную DLP/SIEM/EDR;
- автоматическое предотвращение всех инцидентов;
- юридически гарантированную неизменность evidence.
