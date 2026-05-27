# AWatch-rus: обзор решения для продажи и коммерческого представления

## Executive Summary

`AWatch-rus` — это корпоративная система мониторинга активности сотрудников с DLP-функциями, управленческим слоем и интеграцией в существующий ИТ/ИБ-контур компании.

По сути это практичный средний слой между простыми time-tracker решениями и тяжелыми enterprise DLP-платформами:

- есть контроль действий пользователей и DLP-сигналы;
- есть реальный management layer, а не только сырые события;
- есть интеграции с `1С`, Grafana, Linux-инфраструктурой и forensic follow-up;
- при этом стоимость входа и сопровождения обычно ниже, чем у классических enterprise-комплексов.

Решение особенно уместно там, где:

- есть `Windows` и `RDP`-сценарии;
- важен контроль активности и дисциплины данных;
- уже используется `1С`;
- нужен open-source контур без жесткой привязки к одному вендору.

## Основные возможности

### DLP мониторинг

Система уже собирает и обрабатывает:

- события `clipboard`;
- печать;
- `USB`;
- браузерные домены и web-категории;
- исходящую почту;
- файловые операции;
- DLP-инциденты и review workflow.

### Enforcement

`AWatch-rus` умеет не только наблюдать, но и ограничивать:

- `clipboard block`;
- `USB write-block`;
- отмену печати;
- block path для email в поддерживаемом Outlook-сценарии.

Это дает возможность внедрять контур поэтапно: сначала `monitor`, затем `enforce`.

### Браузеры

Поддерживается:

- сбор доменов и web-контекста;
- категоризация активности;
- связка браузерной телеметрии с DLP и worktime;
- использование данных в dashboard и incident path.

### Email

Поддерживается:

- мониторинг исходящей почты;
- правила `endpoint.email[]`;
- сигналы по теме, адресатам и вложениям;
- блокирующий сценарий в Outlook mode.

### Worktime

Система дает:

- фактический worktime по RDP-сессиям;
- ежедневные отчеты;
- `HTML/CSV/JSON` выдачу;
- server-side management reporting на `:5610`.

### Management Report Layer

Это одна из самых сильных частей решения. Поверх телеметрии строится:

- управленческий отчет;
- очередь действий по сотрудникам и подразделениям;
- source freshness;
- executive summary;
- trend-анализ.

### Интеграция с 1С

Есть отдельный file-based `1С` analytics contour:

- telemetry по файловым базам;
- `ClickHouse`-модели;
- company intelligence;
- manager brief и management actions;
- Grafana boards для руководителя и операционного контура.

### Linux поддержка

Решение не замкнуто только на Windows:

- Linux server-side runtime;
- Linux operational integrations;
- SSH/console logging;
- смешанный Windows/Linux operational model.

### pfSense

Система может быть включена в perimeter/security contour компании через:

- внешний `pfSense` poller;
- передачу network telemetry в общий контур;
- единый operator visibility path.

### Forensic анализ

`Hayabusa` интегрирован как bounded DFIR layer:

- EVTX export с Windows;
- server-side processing;
- case linkage;
- Telegram alert path для follow-up.

Это усиливает ценность решения для ИБ без превращения продукта в отдельную SIEM/DFIR-платформу.

## Управленческие функции

### Management Report Layer

Руководитель получает не просто технические bucket-данные, а:

- картину по активности сотрудников;
- сводку по owner/department;
- проблемные зоны и приоритеты;
- понятную очередь действий.

### Actions с приоритетами

Система умеет формировать:

- `critical/high` actions;
- рекомендации, кого проверять первым;
- причины для escalation;
- управленческий список действий без ручного разбора сырых событий.

### Executive summary

Management API и `1С` management brief формируют human-readable summary уровня:

- что сломалось;
- где данные stale;
- кто не показывает активность;
- какие пользователи и предприятия требуют внимания в первую очередь.

### Trend-анализ

В продукт уже встроены:

- несколько дней тренда по worktime;
- trend и weekly views в `1С` intelligence contour;
- сравнительный анализ текущего и исторического состояния.

### Source freshness

Это критически важная функция для менеджмента и ИБ:

- система показывает, где проблема в поведении пользователя, а где в деградации источника;
- решения не принимаются вслепую по сломанной телеметрии.

### Алиасы пользователей

Поддерживаются:

- normalized user aliases;
- owner/department mapping;
- manager-facing каталоги ответственных.

За счет этого отчеты пригодны для бизнеса, а не только для инженеров.

## Архитектура и компоненты

Архитектура строится как цепочка:

- `Windows Clients / RDP host`;
- `Linux Server`;
- `Integration Layer`;
- `Monitoring Stack`;
- выделенный `Forensic Layer`.

Практически это означает:

- Windows PowerShell collectors;
- Linux `AW-rus` server;
- DLP Policy API и Case API;
- Grafana/Prometheus/ClickHouse analytics;
- Proxmox/operator gateway;
- `Hayabusa` follow-up path.

### Windows коллекторы

В состав входят:

- endpoint DLP collector;
- browser domains collector;
- file operations collector;
- email outbound collector;
- worktime session collector;
- deploy/hardening/validation toolkit.

### Linux сервер

Серверный слой включает:

- `ActivityWatch` API и WebUI;
- RU patch и DLP overlay;
- `aw-worktime-api` на `:5610`;
- policy engine;
- case management;
- health/autoheal path.

### Monitoring стек

Визуализация и наблюдаемость строятся через:

- Grafana;
- Prometheus-compatible monitoring path;
- `1С` analytics dashboards;
- Proxmox Web Gateway как operator entrypoint.

### Proxmox Web Gateway

Gateway дает:

- одну точку входа для операторов и руководства;
- маршруты на Proxmox GUI, AW-rus UI, management pages, Grafana;
- HTTPS access path для внутреннего management contour.

## Преимущества перед конкурентами

### Open-source

- нет vendor lock-in;
- прозрачный код и архитектура;
- можно дорабатывать под процессы заказчика;
- проще аудитировать и сопровождать.

### Легкий агент

- PowerShell collector model;
- нет обязательного тяжелого kernel-level агента;
- легче пилот и проще сопровождение.

### Гибкая DLP политика

- JSON-based policy;
- server-side policy API;
- monitor/enforce режимы;
- адаптация под реальные каналы утечки и корпоративные правила.

### Русификация

- русифицированный WebUI;
- русские Grafana dashboards;
- русская эксплуатационная документация;
- нормальная operator terminology без англоязычного vendor-noise.

### Linux поддержка

- Linux server-side runtime;
- Linux operational integrations;
- гибридный Windows/Linux контур.

### Management Layer

Это сильная дифференциация относительно простых time-tracker решений:

- actions;
- executive summary;
- source freshness;
- owner/department rollups;
- trend и management pages.

### Низкая стоимость владения

По сравнению с классическими enterprise DLP-платформами заказчик получает шанс:

- снизить лицензионную нагрузку;
- не переплачивать за лишний функционал;
- дешевле входить в пилот;
- лучше контролировать стоимость масштабирования.

Корректная подача здесь простая: это не “бесплатная замена любому enterprise DLP”, а прагматичный контур с сильным TCO-профилем.

## Сценарии использования

### Защита от утечек

Подходит, если нужно:

- видеть рискованные действия по `clipboard`, `USB`, печати, email, browser и files;
- фиксировать инциденты;
- в нужных каналах включать block/restrict path.

### Мониторинг продуктивности

Подходит, если компании нужно:

- учитывать активность в RDP;
- получать реальные worktime-данные;
- понимать, кто неактивен по факту, а не по формальному входу в систему.

### Комплаенс 152-ФЗ

Система полезна как practical control/evidence layer:

- DLP incidents;
- compliance reports;
- operator review;
- контроль работы с чувствительными данными.

Это не “автоматическая сертификация”, а инструмент реального operational compliance support.

### Интеграция с 1С

Подходит для компаний, где важно:

- видеть состояние файловых баз;
- понимать активность и риски по предприятиям;
- связывать ИТ, ИБ и управленческий слой.

### Управленческий контроль

Подходит для:

- руководителей подразделений;
- операционных менеджеров;
- ИБ и ИТ, которым нужны единые summary и actions;
- сменных и распределенных управленческих контуров.

### Forensic анализ

Полезен для заказчиков, которым нужен:

- bounded forensic follow-up;
- EVTX-based post-incident path;
- связка инцидента, кейса и расследования в одном operational контуре.

## Технические требования

Базовый practical profile:

- `Windows 10/11` для рабочих станций;
- `Windows Server` / RDP-host сценарии, включая текущий production-target `Windows Server 2025`;
- `Linux` серверный контур на `Debian/Ubuntu`;
- `Docker` для части monitoring/analytics stack;
- `PostgreSQL` и/или другие аналитические БД в интеграционных сценариях;
- `ClickHouse` для file-based `1С` analytics;
- Grafana для визуализации.

Иными словами, продукт не требует exotic stack и нормально ложится в типовую инфраструктуру компании.

## Уровни зрелости продукта

Состояние продукта корректно описывать так:

- operational phases `1-3` по production health, operator path и Windows hardening уже закрыты;
- server-side DLP chain, content-analysis base и docs/release sync уже реализованы;
- maturity по DLP roadmap сейчас выглядит так:
  - `Phase 1` — сделано;
  - `Phase 2` — внедрено частично;
  - `Phase 2.5` enforcement и email outbound — внедрены;
  - `Phase 3+` — дальнейшее развитие policy/correlation/SIEM/advanced analytics.

Roadmap дальше идет в сторону:

- deeper DLP runtime;
- дополнительных regression guards;
- усиления management и integration layer.

То есть продукт уже production-usable, но остается пространством для целевых enterprise-усилений под конкретного заказчика.

## Стоимость и ROI

### Сравнение с enterprise решениями

Типовой enterprise DLP-проект часто означает:

- дорогое лицензирование;
- тяжелый агент;
- длительный rollout;
- дорогое сопровождение изменений.

`AWatch-rus` выигрывает там, где заказчику важны:

- lower entry cost;
- управляемый пилот;
- понятная архитектура;
- возможность адаптации без полной смены платформы.

### Экономия на лицензиях

Корректная коммерческая формулировка такая:

- заказчик потенциально экономит на лицензиях и внедрении по сравнению с тяжелыми enterprise-пакетами;
- итоговая экономия зависит от числа endpoint'ов, объема enforcement, требований к SIEM/SSO/RBAC и объема кастомизации;
- сильная сторона решения — контролируемая стоимость владения, а не обещание “заменить все enterprise DLP в один клик”.

## Поддержка и обучение

Проект уже опирается на:

- подробную русскую документацию;
- runbook и deployment guides;
- Ansible и PowerShell automation;
- community-style support model;
- возможность кастомизации под нужды конкретного заказчика.

Для коммерческого внедрения это означает, что можно предложить:

- пилот;
- rollout;
- обучение операторов и ИБ;
- кастомизацию dashboard, policy и integration path.

## Контакты и следующий шаг

Практический следующий шаг для потенциального заказчика:

1. Провести короткий discovery по инфраструктуре, числу Windows/RDP-host'ов и наличию `1С`.
2. Определить, нужен ли только monitor-mode или сразу важен enforcement path.
3. Выделить пилотный сегмент.
4. Поднять pilot deployment с management report layer и базовым DLP/monitoring контуром.
5. После пилота решить, какие enterprise-усиления действительно нужны, а какие не дадут окупаемого эффекта.

Самая сильная подача продукта простая: не обещать “всё для всех”, а показывать, что `AWatch-rus` уже дает работающий operational control contour с DLP, management и forensic follow-up там, где многие компании либо переплачивают за тяжелые платформы, либо вообще живут без управляемого контроля.
