# Задачи поддержки комплекса DetMir

Документ описывает регулярные и инцидентные работы, которые должны входить в
сопровождение производственного комплекса DetMir на базе AWatch-rus,
ActivityWatch, Windows/RDP-коллекторов, Grafana, ClickHouse, 1C-аналитики и
операторского gateway.

## Цель поддержки

Поддержка должна обеспечивать не формальное состояние сервисов `active`, а
доказуемую работоспособность всего контура: свежие данные в bucket'ах,
корректные отчеты для владельца, рабочие дашборды, доступность gateway,
исправные Windows-задачи и отсутствие накопленных очередей.

## Ежедневные задачи

- Проверять свежесть данных ActivityWatch: AFK, window, worktime sessions,
  web category, DLP endpoint signals, file operations, email, collector guard.
- Проверять состояние RDP-хоста `SHARKON2025`: активные и отключенные сессии,
  зависшие PowerShell-процессы, дубли `aw-watcher-*`, process storm.
- Проверять Windows-задачи и сервисы:
  `AWatchRusCollectorGuard`, `ActivityWatch Recovery`,
  `ActivityWatch Launch [...]`, `ActivityWatch File1C Upload`,
  `ActivityWatch DLP Evidence Sync`, `ActivityWatch Hayabusa Upload`.
- Проверять доступность AW server, worktime API, Grafana, gateway,
  1C manager brief/actions/recovery.
- Проверять ClickHouse-контур: состояние Docker-контейнера, healthcheck,
  сетевую доступность с AW-сервера, ingest timer, свежесть данных.
- Проверять ошибки в логах collector guard, browser collector, DLP,
  file operations, File1C upload, DLP evidence sync и Hayabusa upload.
- Реагировать на `STALE`/`DEAD` bucket'ы точечно: перезапускать конкретный
  collector, scheduled task или guard, а не выполнять массовые рестарты без
  диагностики.
- Проверять, что owner-facing ссылки через gateway открываются штатно и не
  требуют доступа к внутренним портам.

## Еженедельные задачи

- Готовить короткий отчет владельцу: что работало, что ломалось, какие периоды
  данных неполные, что было исправлено.
- Проверять основные Grafana-дашборды и корректность отображения сотрудников.
- Контролировать отсутствие дублей пользователей в отчетах: `USER5/user5`,
  машинные аккаунты, битая кодировка, неверная кириллица.
- Проверять накопление файлов в drop/inbox зонах Hayabusa, File1C и ClickHouse.
- Проверять свободное место, память, pagefile, Docker, nginx, systemd timers и
  журналы ошибок на серверной стороне.
- Проверять, что scheduled tasks имеют ожидаемые интервалы запуска и последние
  результаты без ошибок.
- Просматривать небольшие исправления и обновления, которые можно применить без
  риска простоя.

## Ежемесячные задачи

- Проверять резервные копии Grafana DB, ClickHouse-конфигов и данных,
  ActivityWatch-конфигов, Windows deployment config, SSH-ключей и gateway
  credentials.
- Проводить контрольное восстановление ключевых частей контура: AW server,
  gateway, Grafana, ClickHouse ingest, Windows collectors.
- Выполнять аудит доступов: Basic Auth, SSH, WinRM, открытые порты,
  сертификаты, firewall.
- Обновлять эксплуатационную документацию: IP-адреса, сервисы, task names,
  owner-facing ссылки, команды восстановления.
- Проверять качество перед изменениями: Ansible syntax-check, PowerShell parse,
  Rust tests/build для затронутых компонентов.

## Инцидентные задачи

- Восстанавливать сбор данных после остановки collector'ов или Windows-задач.
- Лечить зависшие RDP-сессии, stale watcher bucket'ы и дубли процессов.
- Перезапускать или пересоздавать конкретные Windows scheduled tasks.
- Устранять process storm, нехватку памяти и проблемы pagefile.
- Восстанавливать DLP endpoint signals и DLP evidence sync.
- Восстанавливать File1C upload, server ingest и ClickHouse health.
- Восстанавливать Hayabusa EVTX upload, drop processing и очереди intake.
- Исправлять Grafana-дашборды, gateway routes и права доступа.
- Проводить ручную валидацию после аварии: bucket freshness, отчеты,
  gateway/Grafana HTTP-коды и отсутствие новых ошибок в логах.

## Изменения и развитие

- Все изменения выполнять по схеме backup-first.
- Перед деплоем проверять синтаксис, тесты и применимость к текущему окружению.
- После деплоя проверять живые endpoints, timers, bucket freshness и
  owner-facing дашборды.
- Не переписывать стабильные компоненты без причины; новые сервисные утилиты
  делать автономными, systemd-friendly и с явными timeout'ами.
- Вести журнал изменений: что изменено, причина, дата, как проверить и как
  откатить.

## Минимальный SLA

- Плановый контроль: один раз в рабочий день.
- Реакция на критичный сбой сбора данных: в течение 2-4 часов.
- Еженедельный отчет владельцу.
- Плановые изменения только с резервной копией и проверкой результата.
- После каждого инцидента: краткое описание причины, выполненных действий и
  мер против повторения.

## Критерии приемки поддержки

- Все ключевые bucket'ы свежие или по каждому stale/dead bucket есть объяснение
  и план восстановления.
- Владелец видит отчеты через защищенный gateway, без доступа к внутренним
  техническим портам.
- Windows/RDP collector'ы работают в правильных пользовательских сессиях.
- ClickHouse, File1C, DLP и Hayabusa не имеют накопленных необработанных
  очередей.
- Любое исправление подтверждается командой проверки, логом или HTTP-кодом, а
  не только статусом systemd/Task Scheduler.
