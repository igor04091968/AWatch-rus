# Ansible ensemble for AWatch-rus

Эта директория содержит Ansible-ensemble для полного развёртывания AWatch-rus:

- деплой на уже существующий Debian host/CT;
- полный цикл с нуля в Proxmox: создание CT + bootstrap + установка ActivityWatch + RU patch;
- централизованное развёртывание Windows/RDP collector'ов по WinRM;
- развёртывание внешнего pfSense poller'а на Debian/Ubuntu utility VM.

## Файлы

- `ansible/deploy_aw_server.yml` — основной playbook для уже существующего Debian/CT host.
- `ansible/provision_proxmox_ct_and_deploy_aw.yml` — полный playbook для Proxmox.
- `ansible/provision_proxmox_ct_matrix_and_deploy_aw.yml` — массовый полный playbook (несколько CT).
- `ansible/deploy_aw_windows.yml` — WinRM playbook для развёртывания Windows/RDP collector'ов.
- `ansible/deploy_aw_pfsense_poller.yml` — развёртывание pfSense poller'а.
- `ansible/install_full_stack.yml` — полный установочный playbook (оркестратор всех этапов).
- `ansible/inventory.example.ini` — шаблон inventory.
- `ansible/group_vars/*.example.yml` — шаблоны переменных.

## Быстрый запуск

1. Скопируйте шаблоны:
   - `cp ansible/inventory.example.ini ansible/inventory.ini`
   - `cp ansible/group_vars/all.example.yml ansible/group_vars/all.yml`
2. Заполните значения в `inventory.ini` и `group_vars/all.yml`.
3. Запустите:

```bash
cd ansible
ansible-playbook -i inventory.ini deploy_aw_server.yml
```

## Полный установочный playbook (всё за один запуск)

Если нужно прогнать полный цикл одной командой:

```bash
cd ansible
ansible-playbook -i inventory.ini install_full_stack.yml
```

Что делает:

- `provision_proxmox_ct_and_deploy_aw.yml` (если есть хосты в группе `[proxmox]`);
- `deploy_aw_server.yml` (группа `[aw_server]`);
- `deploy_aw_windows.yml` (группа `[aw_windows]`);
- `deploy_aw_pfsense_poller.yml` (группа `[aw_pfsense_pollers]`).

Пустые группы в `inventory.ini` безопасны: соответствующий play будет пропущен.

## Полный запуск с нуля в Proxmox

1. Подготовьте inventory и vars:
   - `cp ansible/inventory.example.ini ansible/inventory.ini`
   - `cp ansible/group_vars/all.example.yml ansible/group_vars/all.yml`
   - `cp ansible/group_vars/proxmox.example.yml ansible/group_vars/proxmox.yml`
2. Заполните `group_vars/proxmox.yml` и `group_vars/all.yml`.
3. Запустите playbook:

```bash
cd ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_and_deploy_aw.yml
```

## Массовый запуск (матрица CT)

1. Подготовьте матрицу:
   - `cp ansible/group_vars/proxmox-matrix.example.yml ansible/group_vars/proxmox-matrix.yml`
2. Заполните `proxmox-matrix.yml`.
3. Запустите:

```bash
cd ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_matrix_and_deploy_aw.yml
```

## Windows/RDP rollout (WinRM)

1. Подготовьте inventory и vars:
   - `cp ansible/inventory.example.ini ansible/inventory.ini`
   - `cp ansible/group_vars/windows.example.yml ansible/group_vars/windows.yml`
2. Заполните `inventory.ini` (секция `[aw_windows]`) и `group_vars/windows.yml`.
   - Для русской локализации Windows часто нужен `ansible_user=Администратор` (а не `Administrator`).
   - Если WinRM закрыт, playbook не сможет стартовать и нужно сначала открыть `5985/5986` и `wsman`.
3. Запустите:

```bash
cd ansible
ansible-playbook -i inventory.ini deploy_aw_windows.yml
```

Playbook:

- выгружает полный `windows/*` toolkit на целевой хост в InnoSetup-compatible каталог `C:\Program Files\AWatch-rus\windows`, включая DLP и `worktime-session-collector.ps1`;
- выполняет `deploy-ensemble.ps1` (deploy + hardening/recovery) с policy/rules из AWatch-rus toolkit;
- после deploy принудительно запускает `ActivityWatch Recovery` и все `ActivityWatch Launch *` задачи;
- выполняет API smoke-check bucket `aw-watcher-afk_<COMPUTERNAME>` и ожидает свежие `not-afk` события;
- запускает `validate-deployment.ps1`;
- забирает JSON-отчёт в локальную директорию (`/tmp/aw-rus-validation` по умолчанию).

Дополнительные флаги:

- `aw_windows_afk_enabled: false` — не запускать `aw-watcher-afk`;
- `aw_windows_window_enabled: false` — не запускать `aw-watcher-window`;
- `aw_windows_incident_capture_enabled: false` — отключить блок incidentCapture;
- `aw_windows_incident_screenshot_enabled: false` — не делать скриншот при DLP-инциденте;
- `aw_windows_incident_artifacts_root: 'C:\...\incident-artifacts'` — переопределить путь артефактов;
- `aw_windows_deploy_root: 'C:\Program Files\AWatch-rus'` — каталог toolkit, совпадает с InnoSetup `{app}`;
- `aw_windows_install_root: 'C:\Program Files\AWatch-rus\bin'` — каталог бинарников, совпадает с InnoSetup `AwDefaultInstallRoot`;
- `aw_windows_state_root: 'C:\ProgramData\AWatch-rus'` — каталог состояния/отчётов, совпадает с InnoSetup `AwDefaultStateRoot`;
- `aw_windows_validation_remote_path: '{{ aw_windows_state_root }}\aw_validate_ansible.json'` — отчёт Ansible-валидации хранится рядом с `ensemble-report-*.json`;
- `aw_windows_package_version`, `aw_windows_package_url`, `aw_windows_package_zip_path` — версия и источник Windows-пакета ActivityWatch;
- `aw_windows_api_smoke_check_bucket: ""` — автоматически использовать `aw-watcher-afk_<COMPUTERNAME>`;
- `aw_windows_fail_on_validation_error: true` — завершать playbook ошибкой, если `validate-deployment.ps1` возвращает `overallOk=false`;
- `aw_windows_skip_hardening: true` — пропустить `hardening-recovery.ps1` внутри ensemble-скрипта.

## Развёртывание pfSense poller

1. Подготовьте vars:
   - `cp ansible/group_vars/pfsense-poller.example.yml ansible/group_vars/pfsense-poller.yml`
2. Добавьте inventory group `[aw_pfsense_pollers]`.
3. Запустите:

```bash
cd ansible
ansible-playbook -i inventory.ini deploy_aw_pfsense_poller.yml
```

Playbook:

- ставит `python3`;
- копирует `pfsense-aw-poller.py`;
- пишет `/etc/aw-pfsense/poller.json`;
- поднимает `aw-pfsense-poller.service`.

## Результат

- Установлен ActivityWatch Server.
- Создан systemd-unit `activitywatch-server.service`.
- Установлен RU Web UI patch.
- Для Web UI используется checksum-based cache-bust для `ru-patch-v5.js` и `sw-cleanup.js`, чтобы браузер не держал старую DLP/русскую статику после деплоя.
- На `#/home` Web UI делит хосты на `Windows RDP` и `Virtual servers + Proxmox`.
- Выполнена валидация API `http://127.0.0.1:5600/api/0/info`.
- Для полного сценария CT создаётся автоматически через `pct create`.
- На Windows/RDP host развёрнуты AFK/window watchers, browser domain collector, DLP endpoint collector и worktime session collector.
- Проверочный JSON-отчёт Windows playbook должен иметь `overallOk=true`.
