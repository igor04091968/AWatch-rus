# Onboarding по кодовой базе AWatch-rus

Этот документ — быстрый вход для новичка: **что лежит где**, **как всё связано** и **что читать дальше**.

## 1) Что это за репозиторий

`AWatch-rus` — это не один сервис, а **инфраструктурный набор** для развёртывания и сопровождения ActivityWatch в прод-подобной среде:

- Proxmox/LXC-подготовка,
- установка и настройка AW Server,
- русификация Web UI,
- автоматизация через Ansible,
- клиентский rollout для Windows/Linux,
- дополнительная телеметрия (в т.ч. pfSense poller и DLP-сигналы),
- эксплуатационные runbook/operations документы.

Идея: чтобы развёртывание было **повторяемым**, а не «ручной магией в один вечер».

## 2) Карта проекта (по папкам)

### `docs/`
Главный источник истины по процессам.

- `preparation.md` — входные данные, prerequisites, что нужно до старта.
- `deployment.md` — базовый серверный деплой.
- `runbook.md` — быстрые операционные действия и проверки.
- `operations.md` — сопровождение, бэкапы, rollback.
- `windows/*` — отдельная ветка документации по Windows-оркестрации.
- `1C_GRAFANA_DEPLOYMENT_RU.md`, `pfsense.md`, `linux-client.md` — специализированные подсистемы.

### `proxmox/`
Скрипты ранней инфраструктурной фазы:

- `create-ct.sh` — создание/подготовка контейнера,
- `push-aw-artifacts.sh` — доставка артефактов и конфигов.

### `aw-server/`
Серверная «сердцевина»:

- `install_aw_server.sh` — установка AW Server,
- `apply_webui_ru_patch.sh` + `aw-ru-patch.js` — русификация UI,
- `activitywatch-server.service` — unit для systemd,
- `aw-server.env.example` — шаблон переменных окружения,
- `settings/*.json` — конфигурация представлений/классов.

### `ansible/`
Идемпотентная автоматизация (вместо ручных команд):

- playbook'и для серверного деплоя,
- сценарии provisioning + deploy,
- `group_vars/*.example.yml` и `inventory.example.ini` как шаблоны входных данных.

### `windows/`
PowerShell toolkit для клиентской стороны:

- `deploy-ensemble.ps1` — оркестратор,
- `deploy-single-user.ps1`, `deploy-domain-users.ps1` — сценарии установки,
- `validate-deployment.ps1` — post-check,
- `ActivityWatch.Windows.Common.psm1` — общая библиотека функций,
- скрипты DLP/browser telemetry.

### `pfsense/`
Отдельный poller для pfSense API + systemd unit.

### `grafana-1c/`
Набор для SQL exporter + Prometheus + Grafana дашбордов по 1C метрикам.

### `scripts/`
Утилиты и quality gates:

- `quality-gate.sh` — базовый preflight,
- инсталляторы Linux-клиента и console/ssh logger режимов.

### `secrets/`
Только шаблоны. Реальные секреты в репозиторий не кладутся.

## 3) Как компоненты связаны в потоке

Типовой pipeline:

1. Подготовка параметров (`docs/preparation.md`, `secrets/*.example`).
2. Provisioning контейнера в Proxmox (`proxmox/`).
3. Установка/настройка AW Server (`aw-server/`).
4. Включение автозапуска и проверка (`systemd` + `docs/runbook.md`).
5. Rollout клиентов (обычно `windows/`, иногда `scripts/install_aw_linux_client.sh`).
6. Эксплуатация и изменения через `docs/operations.md`.
7. При необходимости — расширение мониторинга (`grafana-1c/`, `pfsense/`).

## 4) Что важно понять в первую очередь

1. **Репозиторий документ-ориентированный**: сначала читаешь `docs/`, потом запускаешь скрипты.
2. **Шаблоны `.example` — обязательная точка входа**: не редактируй скрипты вместо заполнения переменных.
3. **Есть два режима работы**:
   - ручной/полуручной (bash + runbook),
   - автоматизированный (Ansible).
4. **Windows часть — полноценный под-проект** с собственными deploy/validate практиками.
5. **Безопасность**: никаких секретов в git; rollback и backup — не опция, а стандарт процесса.

## 5) Рекомендованный порядок изучения (первые 2–3 часа)

1. `README.md` — получить общую картину.
2. `docs/preparation.md` — понять входные параметры.
3. `docs/deployment.md` — увидеть «сквозной» серверный сценарий.
4. `docs/runbook.md` и `docs/operations.md` — как жить с системой после деплоя.
5. `aw-server/install_aw_server.sh` и `aw-server/activitywatch-server.service` — как реально стартует сервис.
6. `windows/deploy-ensemble.ps1` + `windows/validate-deployment.ps1` — клиентская фаза.
7. `ansible/README.md` и ключевые playbook'и — переход к промышленной автоматизации.

## 6) Практические подсказки для первого вклада

- Начни с правок документации или `.example`-шаблонов — это самый безопасный вход.
- Перед изменениями в скриптах сравни, нет ли уже Ansible-аналога (лучше поддерживать один «официальный» путь).
- Любая новая переменная должна быть отражена:
  1) в `.example` файле,
  2) в docs,
  3) в проверках/валидации (если применимо).
- Для Windows-скриптов используй общие функции из `ActivityWatch.Windows.Common.psm1`, чтобы не дублировать логику.

## 7) Куда смотреть дальше (углубление)

- Если интересует эксплуатация и инциденты: `docs/runbook.md`, `docs/operations.md`, `docs/windows/troubleshooting.md`.
- Если интересует автодеплой: `ansible/provision_proxmox_ct_and_deploy_aw.yml` и `ansible/tasks/`.
- Если интересует наблюдаемость: `grafana-1c/` + `pfsense/` + `prometheus`/`alerts` конфиги.
- Если интересует hardening и DLP: `windows/hardening-recovery.ps1`, `docs/dlp-gap-analysis.md`.

---

Если ты новичок в проекте, практичный старт: разверни тестовый стенд по `docs/deployment.md`, затем прогоняй валидации из `docs/runbook.md` и `windows/validate-deployment.ps1`.
