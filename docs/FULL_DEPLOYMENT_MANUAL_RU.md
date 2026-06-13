Полная инструкция по развёртыванию и поддержке AWatch-rus

Статус документа

Этот документ описывает актуальный **Rust-fiWindows/PowerShell deployment flow больше не считается основным способом развёртывания, патчинга или эксплуатации. Если в репозитории остаются старые ".ps1"-файлы, они рассматриваются как legacy/history или как будущий provider-слой, но не как production runtime.

0. Назначение

AWatch-rus — программный комплекс операционного контроля, технического аудита, оценки трудоотдачи сотрудников и мониторинга корпоративной ИТ-инфраструктуры на базе:

- Rust backend/runtime;
- Rust Agent;
- Rust server-rendered HTML + HTMX-compatible JSON API;
- Grafana/Prometheus-витрин;
- модулей Workforce, Security и Forensics;
- evidence/reporting tooling;
- ActivityWatch-compatible источников данных, где это применимо.

Проект не позиционируется как сертифицированная DLP/SIEM/EDR/XDR/СЗИ. DLP, evidence, UEBA и расследовательские функции используются как внутренние аналитические и операционные модули.

1. Актуальная архитектура

1.1 Основной runtime

Основной production runtime AWatch-rus — Rust-first:

- backend/runtime — Rust;
- agent — Rust;
- portal — Rust server-rendered HTML + HTMX-compatible JSON API;
- operational status/check — Rust;
- DLP server-side helpers — Rust;
- worktime helpers/exporters/prewarm — Rust;
- SLO/health/readiness helpers — Rust;
- evidence/install-kit tooling — Rust;
- auto-heal helpers — Rust, только в безопасном режиме.

1.2 Что не является основным runtime

Не считать основным production deployment flow:

- PowerShell deployment;
- старые Windows ".ps1" rollout scripts;
- ручное исправление production-файлов без release/backup;
- прямое редактирование Web UI в "/opt" без воспроизводимого патча;
- Python/shell как основной operational runtime, если для компонента уже есть Rust-аналог.

Python, shell, Ansible или PowerShell могут оставаться в проекте только как:

- legacy compatibility;
- вспомогательные dev/test tools;
- миграционные сценарии;
- будущие provider-слои;
- Telegram/OCR/AI/ETL/MCP helpers, если они явно не входят в Rust-first core.

2. Типовые роли узлов

2.1 Server node

Серверный узел содержит:

- AWatch-rus backend/runtime;
- portal;
- API;
- exporters;
- health/readiness/status tooling;
- systemd units/timers;
- Grafana/Prometheus integration;
- evidence/reporting storage.

2.2 Agent node

Agent node содержит:

- Rust Agent;
- локальную конфигурацию агента;
- systemd service или другой штатный supervisor;
- локальные логи;
- буфер/очередь, если предусмотрено конфигурацией;
- сетевой доступ до backend/API.

2.3 Monitoring node

Monitoring node может содержать:

- Prometheus;
- Grafana;
- dashboards;
- alerting rules;
- external logs/metrics storage.

Monitoring node может совпадать с server node в пилотной установке.

3. Требования

3.1 Базовые требования

- Linux-сервер или LXC/VM.
- Доступ администратора к systemd.
- Rust toolchain для сборочного узла.
- Сетевой доступ между agent node и server node.
- Закрытый доступ к API и порталу через VPN, reverse proxy или внутренний контур.
- Backup/snapshot перед любым production patch.

3.2 Рекомендуемый production-подход

Для production не собирать проект прямо на боевом сервере, если есть отдельный build host.

Рекомендуемый поток:

git checkout нужного commit/tag
→ cargo fmt / clippy / test / build
→ упаковка release artifacts
→ перенос artifacts на сервер
→ backup/snapshot
→ остановка/перезапуск нужных services
→ smoke tests
→ фиксация версии

4. Основные пути

Рекомендуемая структура на сервере:

/opt/awatch-rus/
  bin/
  etc/
  portal/
  releases/
  evidence/
  reports/
  logs/

/etc/awatch-rus/
  awatch-rus.env
  agent.env
  portal.env

/var/lib/awatch-rus/
  data/
  state/
  cache/
  evidence/
  reports/

/var/log/awatch-rus/
  backend.log
  agent.log
  portal.log
  exporter.log

Рекомендуемые runtime binaries:

/usr/local/bin/detmir-status
/usr/local/bin/detmir-check
/usr/local/bin/detmir-dlp
/usr/local/bin/detmir-auto
/usr/local/bin/detmir-heal-safe
/usr/local/bin/aw-rus-healthd

Имена конкретных бинарников должны соответствовать текущему "Cargo.toml" и фактически собранным artifacts. Если имя binary изменено, документация и systemd unit должны обновляться в том же commit.

5. Конфигурация

5.1 Общие правила

- Не хранить production secrets в публичном репозитории.
- Не коммитить реальные hostnames, IP, логины, ФИО, токены, пароли.
- Для production использовать "/etc/awatch-rus/*.env".
- Для demo использовать только обезличенные fixtures.
- Все параметры, влияющие на runtime, должны быть описаны в документации.

5.2 Пример server env

AWATCH_ENV=production
AWATCH_BIND_ADDR=127.0.0.1
AWATCH_PORT=5600
AWATCH_DATA_DIR=/var/lib/awatch-rus/data
AWATCH_LOG_DIR=/var/log/awatch-rus
AWATCH_EVIDENCE_DIR=/var/lib/awatch-rus/evidence
AWATCH_REPORTS_DIR=/var/lib/awatch-rus/reports
RUST_LOG=info

5.3 Пример agent env

AWATCH_AGENT_ENV=production
AWATCH_SERVER_URL=https://awatch.example.local
AWATCH_AGENT_HOST_ID=HOSTNAME_OR_NODE_ID
AWATCH_AGENT_DATA_DIR=/var/lib/awatch-rus/agent
AWATCH_AGENT_LOG_DIR=/var/log/awatch-rus
RUST_LOG=info

6. Сборка

6.1 Проверки перед сборкой

На build host:

cd /path/to/AWatch-rus

git status --short
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

Если в репозитории есть проектные quality gates, выполнить их обязательно:

bash scripts/check_private_config_guard.sh
bash scripts/quality-gate.sh

Если какой-то скрипт отсутствует в текущей ветке, не создавать фиктивную замену. Зафиксировать это в release notes.

6.2 Release build

cargo build --release --workspace

Проверить artifacts:

find target/release -maxdepth 1 -type f -executable -print

6.3 Упаковка artifacts

Рекомендуемый вариант:

mkdir -p dist/awatch-rus-release/bin
cp target/release/detmir-status dist/awatch-rus-release/bin/ 2>/dev/null || true
cp target/release/detmir-check dist/awatch-rus-release/bin/ 2>/dev/null || true
cp target/release/detmir-dlp dist/awatch-rus-release/bin/ 2>/dev/null || true
cp target/release/detmir-auto dist/awatch-rus-release/bin/ 2>/dev/null || true
cp target/release/detmir-heal-safe dist/awatch-rus-release/bin/ 2>/dev/null || true
cp target/release/aw-rus-healthd dist/awatch-rus-release/bin/ 2>/dev/null || true

tar -C dist -czf awatch-rus-release.tar.gz awatch-rus-release
sha256sum awatch-rus-release.tar.gz > awatch-rus-release.tar.gz.sha256

Не использовать "cp ... || true" в CI без последующей проверки обязательных binaries. Для ручного production release список обязательных binaries должен быть проверен явно.

7. Первичное развёртывание server node

7.1 Создание каталогов

sudo mkdir -p /opt/awatch-rus/bin
sudo mkdir -p /opt/awatch-rus/releases
sudo mkdir -p /etc/awatch-rus
sudo mkdir -p /var/lib/awatch-rus/data
sudo mkdir -p /var/lib/awatch-rus/state
sudo mkdir -p /var/lib/awatch-rus/evidence
sudo mkdir -p /var/lib/awatch-rus/reports
sudo mkdir -p /var/log/awatch-rus

7.2 Установка binaries

sudo install -m 0755 dist/awatch-rus-release/bin/* /opt/awatch-rus/bin/

Создать symlink для удобства:

sudo ln -sf /opt/awatch-rus/bin/detmir-status /usr/local/bin/detmir-status
sudo ln -sf /opt/awatch-rus/bin/detmir-check /usr/local/bin/detmir-check
sudo ln -sf /opt/awatch-rus/bin/detmir-dlp /usr/local/bin/detmir-dlp

Если binary отсутствует, не создавать пустой symlink. Сначала проверить фактический состав release artifact.

7.3 Конфигурация

sudo install -m 0640 awatch-rus.env /etc/awatch-rus/awatch-rus.env

Проверить права:

sudo chown root:root /etc/awatch-rus/awatch-rus.env
sudo chmod 0640 /etc/awatch-rus/awatch-rus.env

8. systemd units

8.1 Пример backend service

[Unit]
Description=AWatch-rus backend/runtime
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/awatch-rus/awatch-rus.env
ExecStart=/opt/awatch-rus/bin/awatch-rus-backend
Restart=on-failure
RestartSec=5
WorkingDirectory=/opt/awatch-rus
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=full
ProtectHome=true
ReadWritePaths=/var/lib/awatch-rus /var/log/awatch-rus

[Install]
WantedBy=multi-user.target

Если фактическое имя backend binary отличается, заменить "awatch-rus-backend" на актуальное имя из release artifact.

8.2 Пример health service

[Unit]
Description=AWatch-rus health daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/awatch-rus/awatch-rus.env
ExecStart=/opt/awatch-rus/bin/aw-rus-healthd
Restart=on-failure
RestartSec=5
WorkingDirectory=/opt/awatch-rus
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=full
ProtectHome=true
ReadWritePaths=/var/lib/awatch-rus /var/log/awatch-rus

[Install]
WantedBy=multi-user.target

8.3 Применение unit files

sudo systemctl daemon-reload
sudo systemctl enable --now awatch-rus-backend.service
sudo systemctl enable --now aw-rus-healthd.service

Если конкретный unit не используется в текущей инсталляции, не создавать фиктивный сервис. Документировать фактический набор services.

9. Развёртывание Rust Agent

9.1 Установка agent binary

sudo mkdir -p /opt/awatch-rus/bin
sudo mkdir -p /etc/awatch-rus
sudo mkdir -p /var/lib/awatch-rus/agent
sudo mkdir -p /var/log/awatch-rus

sudo install -m 0755 awatch-rus-agent /opt/awatch-rus/bin/awatch-rus-agent
sudo install -m 0640 agent.env /etc/awatch-rus/agent.env

9.2 Пример agent service

[Unit]
Description=AWatch-rus Rust Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/awatch-rus/agent.env
ExecStart=/opt/awatch-rus/bin/awatch-rus-agent
Restart=on-failure
RestartSec=5
WorkingDirectory=/opt/awatch-rus
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=full
ProtectHome=true
ReadWritePaths=/var/lib/awatch-rus /var/log/awatch-rus

[Install]
WantedBy=multi-user.target

9.3 Запуск agent

sudo systemctl daemon-reload
sudo systemctl enable --now awatch-rus-agent.service
sudo systemctl status awatch-rus-agent.service --no-pager

10. Развёртывание портала

Портальный слой AWatch-rus зафиксирован как Rust server-rendered HTML + HTMX-compatible JSON API.

10.1 Общий порядок

build portal/backend binary
→ install binary
→ install templates/static assets, если они выделены отдельно
→ update portal env
→ restart portal service
→ smoke check HTTP/API routes

10.2 Проверка портала

curl -fsS http://127.0.0.1:5600/healthz
curl -fsS http://127.0.0.1:5600/readyz
curl -fsS http://127.0.0.1:5600/version

Если конкретные endpoints в текущей версии отличаются, использовать фактически реализованные health/readiness/version endpoints и обновить этот документ в том же commit.

11. Патчи в развернутой среде

11.1 Правило

Любой production patch применяется только через контролируемый цикл:

определить commit/tag
→ собрать release artifact
→ выполнить локальные проверки
→ сделать backup/snapshot
→ установить новые binaries/configs
→ restart/reload services
→ smoke tests
→ зафиксировать результат
→ сохранить rollback path

11.2 Перед патчем

git rev-parse HEAD
git status --short

Сохранить:

дата/время
commit/tag
кто применяет
какие services затрагиваются
какой rollback path

11.3 Backup перед патчем

Если используется Proxmox/LXC:

vzdump <CT_ID> --mode snapshot --compress zstd --storage <BACKUP_STORAGE>

Внутри сервера:

sudo tar -C / -czf /root/awatch-rus-backup-$(date +%Y%m%d-%H%M%S).tgz \
  etc/awatch-rus \
  opt/awatch-rus \
  var/lib/awatch-rus \
  var/log/awatch-rus

Если данные большие, backup "/var/lib/awatch-rus" выполнять отдельной процедурой согласно backup policy.

11.4 Установка нового binary

Сохранить предыдущую версию:

sudo mkdir -p /opt/awatch-rus/releases/previous
sudo cp -a /opt/awatch-rus/bin /opt/awatch-rus/releases/previous/bin-$(date +%Y%m%d-%H%M%S)

Установить новый artifact:

sudo install -m 0755 dist/awatch-rus-release/bin/* /opt/awatch-rus/bin/

11.5 Restart services

sudo systemctl daemon-reload
sudo systemctl restart awatch-rus-backend.service
sudo systemctl restart aw-rus-healthd.service

Если патч касается только agent:

sudo systemctl restart awatch-rus-agent.service

Если сервис в текущем контуре называется иначе, использовать фактическое имя systemd unit.

12. Smoke-тесты после патча

12.1 Systemd

systemctl --failed --no-pager
systemctl status awatch-rus-backend.service --no-pager
systemctl status aw-rus-healthd.service --no-pager

12.2 Rust operational checks

detmir-status --json
detmir-check --json
detmir-dlp --json

Если отдельная команда не установлена в данном контуре, это не считается ошибкой только при наличии документированного исключения.

12.3 HTTP/API

curl -fsS http://127.0.0.1:5600/healthz
curl -fsS http://127.0.0.1:5600/readyz
curl -fsS http://127.0.0.1:5600/version

12.4 Portal smoke

Проверить в браузере:

/portal
/portal/reports
/portal/architecture

Для Pilot v1 проверить роли:

executive
manager
security
forensics
admin

12.5 Data freshness

Проверить, что витрины и отчёты не пустые из-за сбоя сбора:

последние события поступают
worktime reports обновляются
DLP/security events отображаются, если включены
evidence/reporting не падает
Grafana dashboards открываются

13. Rollback

13.1 Быстрый rollback binary

Найти предыдущий backup:

ls -lah /opt/awatch-rus/releases/previous/

Восстановить:

sudo rsync -a --delete /opt/awatch-rus/releases/previous/bin-YYYYMMDD-HHMMSS/ /opt/awatch-rus/bin/
sudo systemctl restart awatch-rus-backend.service
sudo systemctl restart aw-rus-healthd.service

13.2 Rollback конфигурации

sudo cp /etc/awatch-rus/awatch-rus.env.bak /etc/awatch-rus/awatch-rus.env
sudo systemctl restart awatch-rus-backend.service

13.3 Rollback CT/VM

Если повреждение затрагивает runtime, данные или systemd-конфигурацию:

остановить сервисы
восстановить snapshot/backup
проверить health/readiness/version
проверить портал
проверить поступление данных
зафиксировать incident note

14. Monitoring

14.1 Что должно контролироваться

- service status;
- process uptime;
- API health/readiness;
- latency;
- error rate;
- freshness данных;
- заполненность диска;
- размер логов;
- успешность exporters;
- SLO status;
- agent coverage;
- отсутствие failed systemd units.

14.2 Grafana

В Grafana должны быть разделены витрины:

- executive dashboard;
- security dashboard;
- operations dashboard;
- RDP/user activity dashboard;
- data quality/freshness dashboard;
- DLP/evidence dashboard, если модуль включён.

14.3 Prometheus

Prometheus scrape должен быть доступен только из внутреннего контура мониторинга. Не открывать metrics endpoints наружу.

15. Security hardening

Обязательные правила:

- не публиковать API напрямую в интернет;
- использовать VPN/reverse proxy/access control;
- закрыть лишние порты;
- хранить secrets вне git;
- ограничить права systemd services;
- использовать отдельного service user, если это поддерживается текущей установкой;
- включить backup;
- проверять логи после каждого патча;
- не использовать demo fixtures как production data;
- не смешивать реальные ФИО/IP/hostname с публичными demo screenshots.

16. Проверка перед вводом в эксплуатацию

Минимальный checklist:

[ ] выбран commit/tag release
[ ] cargo fmt прошёл
[ ] cargo clippy прошёл
[ ] cargo test прошёл
[ ] cargo build --release прошёл
[ ] private config guard прошёл
[ ] backup/snapshot создан
[ ] binaries установлены
[ ] systemd services запущены
[ ] health/readiness/version отвечают
[ ] detmir-status/check/dlp работают
[ ] portal открывается
[ ] роли Pilot v1 проверены
[ ] Grafana dashboards открываются
[ ] данные поступают
[ ] rollback path известен
[ ] дата/commit/оператор зафиксированы

17. Что больше не использовать как основной путь

Не использовать как основной production flow:

windows/deploy-single-user.ps1
windows/deploy-domain-users.ps1
windows/deploy-ensemble.ps1
windows/validate-deployment.ps1
windows/hardening-recovery.ps1
windows/browser-domains-native-collector.ps1
windows/dlp-endpoint-signals-collector.ps1

Если эти файлы физически остаются в репозитории, они должны быть явно помечены как:

legacy
planned provider
migration-only
dev/test helper

Они не должны описываться в основном deployment manual как обязательный production-путь.

18. Короткий production runbook

18.1 Развернуть

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace

sudo install -m 0755 target/release/<binary> /opt/awatch-rus/bin/<binary>
sudo systemctl daemon-reload
sudo systemctl restart <service>.service

18.2 Проверить

systemctl --failed --no-pager
detmir-status --json
detmir-check --json
curl -fsS http://127.0.0.1:5600/healthz
curl -fsS http://127.0.0.1:5600/readyz
curl -fsS http://127.0.0.1:5600/version

18.3 Откатить

sudo rsync -a --delete /opt/awatch-rus/releases/previous/bin-YYYYMMDD-HHMMSS/ /opt/awatch-rus/bin/
sudo systemctl restart <service>.service

19. Правило актуализации этого документа

Если меняется:

- имя binary;
- имя systemd unit;
- порт;
- endpoint;
- путь хранения данных;
- способ сборки;
- способ доставки artifacts;
- smoke-test;
- rollback procedure;

то этот файл должен обновляться в том же commit, что и изменение кода или deployment-конфигурации.
