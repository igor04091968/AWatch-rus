# Ansible ensemble for AWatch-rus

Эта директория содержит Ansible-ensemble для двух сценариев:

- деплой на уже существующий Debian host/CT;
- полный цикл с нуля в Proxmox: создание CT + bootstrap + установка ActivityWatch + RU patch.
- централизованный деплой Windows phase-2 collectors по WinRM.
- deployment внешнего pfSense poller'а на Debian/Ubuntu utility VM.

## Файлы

- `/home/igor/tmp/AWatch-rus/ansible/deploy_aw_server.yml` — основной playbook.
- `/home/igor/tmp/AWatch-rus/ansible/provision_proxmox_ct_and_deploy_aw.yml` — full-stack playbook для Proxmox.
- `/home/igor/tmp/AWatch-rus/ansible/provision_proxmox_ct_matrix_and_deploy_aw.yml` — массовый full-stack playbook (несколько CT).
- `/home/igor/tmp/AWatch-rus/ansible/deploy_aw_windows_phase2.yml` — WinRM playbook для развёртывания phase-2 Windows collector'ов.
- `/home/igor/tmp/AWatch-rus/ansible/deploy_aw_pfsense_poller.yml` — deployment pfSense poller'а.
- `/home/igor/tmp/AWatch-rus/ansible/inventory.example.ini` — шаблон inventory.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/all.example.yml` — шаблон переменных.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/proxmox.example.yml` — шаблон переменных CT в Proxmox.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/proxmox-matrix.example.yml` — шаблон матрицы CT.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/windows.example.yml` — шаблон переменных Windows phase-2.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/pfsense-poller.example.yml` — шаблон переменных pfSense poller'а.

## Быстрый запуск

1. Скопируйте шаблоны:
   - `cp /home/igor/tmp/AWatch-rus/ansible/inventory.example.ini /home/igor/tmp/AWatch-rus/ansible/inventory.ini`
   - `cp /home/igor/tmp/AWatch-rus/ansible/group_vars/all.example.yml /home/igor/tmp/AWatch-rus/ansible/group_vars/all.yml`
2. Заполните значения в `inventory.ini` и `group_vars/all.yml`.
3. Запустите:

```bash
cd /home/igor/tmp/AWatch-rus/ansible
ansible-playbook -i inventory.ini deploy_aw_server.yml
```

## Полный запуск с нуля в Proxmox

1. Подготовьте inventory и vars:
   - `cp /home/igor/tmp/AWatch-rus/ansible/inventory.example.ini /home/igor/tmp/AWatch-rus/ansible/inventory.ini`
   - `cp /home/igor/tmp/AWatch-rus/ansible/group_vars/all.example.yml /home/igor/tmp/AWatch-rus/ansible/group_vars/all.yml`
   - `cp /home/igor/tmp/AWatch-rus/ansible/group_vars/proxmox.example.yml /home/igor/tmp/AWatch-rus/ansible/group_vars/proxmox.yml`
2. Заполните `group_vars/proxmox.yml` и `group_vars/all.yml`.
3. Запустите playbook:

```bash
cd /home/igor/tmp/AWatch-rus/ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_and_deploy_aw.yml
```

## Массовый запуск (матрица CT)

1. Подготовьте матрицу:
   - `cp /home/igor/tmp/AWatch-rus/ansible/group_vars/proxmox-matrix.example.yml /home/igor/tmp/AWatch-rus/ansible/group_vars/proxmox-matrix.yml`
2. Заполните `proxmox-matrix.yml`.
3. Запустите:

```bash
cd /home/igor/tmp/AWatch-rus/ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_matrix_and_deploy_aw.yml
```

## Windows phase-2 rollout (WinRM)

1. Подготовьте inventory и vars:
   - `cp /home/igor/tmp/AWatch-rus/ansible/inventory.example.ini /home/igor/tmp/AWatch-rus/ansible/inventory.ini`
   - `cp /home/igor/tmp/AWatch-rus/ansible/group_vars/windows.example.yml /home/igor/tmp/AWatch-rus/ansible/group_vars/windows.yml`
2. Заполните `inventory.ini` (секция `[aw_windows]`) и `group_vars/windows.yml`.
   - Для русской локализации Windows часто нужен `ansible_user=Администратор` (а не `Administrator`).
   - Если WinRM закрыт, playbook не сможет стартовать и нужно сначала открыть `5985/5986` и `wsman`.
3. Запустите:

```bash
cd /home/igor/tmp/AWatch-rus/ansible
ansible-playbook -i inventory.ini deploy_aw_windows_phase2.yml
```

Playbook:

- выгружает `windows/*` toolkit на целевой хост в `C:\Deploy\AWatch-rus\windows`;
- выполняет `deploy-ensemble.ps1` (deploy + hardening/recovery) с phase-2 policy/rules;
- после deploy принудительно запускает `ActivityWatch Recovery` и все `ActivityWatch Launch *` задачи;
- выполняет API smoke-check bucket `aw-watcher-afk_SHARKON2025` и ожидает свежие `not-afk` события;
- запускает `validate-deployment.ps1`;
- забирает JSON-отчёт в локальную директорию (`/tmp/aw-rus-validation` по умолчанию).

Дополнительные флаги:

- `aw_windows_afk_enabled: false` — не запускать `aw-watcher-afk`;
- `aw_windows_window_enabled: false` — не запускать `aw-watcher-window`;
- `aw_windows_incident_capture_enabled: false` — отключить блок incidentCapture;
- `aw_windows_incident_screenshot_enabled: false` — не делать скриншот при DLP-инциденте;
- `aw_windows_incident_artifacts_root: 'C:\...\incident-artifacts'` — переопределить путь артефактов;
- `aw_windows_skip_hardening: true` — пропустить `hardening-recovery.ps1` внутри ensemble-скрипта.

## pfSense poller rollout

1. Подготовьте vars:
   - `cp /home/igor/tmp/AWatch-rus/ansible/group_vars/pfsense-poller.example.yml /home/igor/tmp/AWatch-rus/ansible/group_vars/pfsense-poller.yml`
2. Добавьте inventory group `[aw_pfsense_pollers]`.
3. Запустите:

```bash
cd /home/igor/tmp/AWatch-rus/ansible
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
- Для full-stack сценария CT создаётся автоматически через `pct create`.
