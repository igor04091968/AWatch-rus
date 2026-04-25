# Полная инструкция по развёртыванию и поддержке ActivityWatch-Russian

Документ описывает полный цикл: Proxmox/LXC сервер, установка ActivityWatch Server, RU Web UI patch, развёртывание Windows-клиентов в другом AD-домене, валидация, сопровождение и rollback.

---

## 0) Структура проекта (полные пути)

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/create-ct.sh`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/push-aw-artifacts.sh`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/aw-server/install_aw_server.sh`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/aw-server/apply_webui_ru_patch.sh`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/windows/deploy-single-user.ps1`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/windows/deploy-domain-users.ps1`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/windows/deploy-ensemble.ps1`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/windows/hardening-recovery.ps1`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/windows/validate-deployment.ps1`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/windows/browser-domains-native-collector.ps1`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/ansible/deploy_aw_server.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/ansible/provision_proxmox_ct_and_deploy_aw.yml`

---

## 1) Подготовка

### 1.1 Требования

- Proxmox VE 8/9, доступ root (или sudo с правами на `pct`).
- Шаблон Debian 12 LXC на хосте Proxmox.
- Windows хост(ы) с PowerShell 5.1+ и правами локального администратора.
- Сетевой доступ Windows-клиентов до ActivityWatch Server (`5600/tcp`).

### 1.2 Подготовка единого файла секретов

Скопируйте шаблон:

```bash
cp /mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env.example \
   /mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env
```

Заполните в файле `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env`:

- все `CT_*` параметры контейнера;
- все `AW_SERVER_*` параметры сервера;
- `CT_PASSWORD` (реальный пароль).

Важно: этот файл подхватывается автоматически скриптами Proxmox.

---

## 2) Развёртывание сервера в Proxmox

### 2.0 Ansible full-stack (создание CT + установка AW)

Подготовьте:

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/ansible/inventory.ini`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/ansible/group_vars/all.yml`
- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/ansible/group_vars/proxmox.yml`

Запуск:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_and_deploy_aw.yml
```

Этот сценарий полностью закрывает:

- создание CT в Proxmox;
- bootstrap пакетов в CT;
- установку ActivityWatch Server;
- применение RU Web UI patch;
- проверку API.

### 2.1 Создать LXC контейнер

На узле Proxmox:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/create-ct.sh
```

По умолчанию читается:

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env`

При необходимости можно передать другой путь:

```bash
/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/create-ct.sh /absolute/path/to/deploy.secrets.env
```

### 2.2 Загрузить bootstrap-артефакты и env внутрь CT

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian
/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/push-aw-artifacts.sh
```

Скрипт загружает в CT:

- `/root/bootstrap/install_aw_server.sh`
- `/root/bootstrap/apply_webui_ru_patch.sh`
- `/root/bootstrap/activitywatch-server.service`
- `/root/bootstrap/aw-ru-patch.js`
- `/root/bootstrap/aw-sw-cleanup.js`
- `/etc/activitywatch/aw-server.env` (из `AW_SERVER_*`)

### 2.3 Установить ActivityWatch Server внутри CT

```bash
pct enter <CT_ID>
bash /root/bootstrap/install_aw_server.sh
```

### 2.4 Применить RU patch Web UI

```bash
bash /root/bootstrap/apply_webui_ru_patch.sh
systemctl restart activitywatch-server.service
```

### 2.5 Проверка сервера

В CT:

```bash
systemctl status activitywatch-server.service --no-pager
curl -fsS http://127.0.0.1:5600/api/0/info
ss -ltnp | grep 5600
grep -n 'aw-ru-patch\|aw-sw-cleanup' /opt/activitywatch/webui-ru/index.html
```

Ожидается:

- сервис `active (running)`;
- API отвечает JSON;
- порт 5600 слушается;
- в `index.html` присутствуют оба скрипта.

---

## 3) Развёртывание Windows-клиентов (другой AD-домен)

### 3.1 Подготовка на Windows-хосте

Скопируйте каталог:

- `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/windows`

например в:

- `C:\Deploy\ActivityWatch-Russian\windows`

Откройте **elevated PowerShell**:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope Process
```

### 3.2 Массовое доменное развёртывание (рекомендуется)

Пример со списком пользователей:

```powershell
C:\Deploy\ActivityWatch-Russian\windows\deploy-domain-users.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -Domain CONTOSO `
  -UserListPath C:\Deploy\aw-users.txt `
  -CustomRulesPath C:\Deploy\ActivityWatch-Russian\windows\web-category-rules.example.json
```

Поддерживаемые варианты:

- `-Users user01,user02`
- `-Users 'CONTOSO\user01','CONTOSO\user02'`
- `-UserListPath <txt|csv>`

### 3.2.1 Ensemble orchestration (рекомендуется для production)

```powershell
C:\Deploy\ActivityWatch-Russian\windows\deploy-ensemble.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -Domain CONTOSO `
  -Users user1,user2,user3,user4,user5 `
  -ValidateAfterDeploy
```

Отчёт сохраняется в:

- `C:\ProgramData\ActivityWatch\ensemble-report-YYYYMMDD-HHMMSS.json`

### 3.3 Single-user развёртывание

```powershell
C:\Deploy\ActivityWatch-Russian\windows\deploy-single-user.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -TargetUser 'CONTOSO\user01' `
  -CustomRulesPath C:\Deploy\ActivityWatch-Russian\windows\web-category-rules.example.json
```

### 3.4 Recovery / hardening

```powershell
C:\Deploy\ActivityWatch-Russian\windows\hardening-recovery.ps1 `
  -ConfigPath C:\ProgramData\ActivityWatch\deployment-config.json
```

### 3.5 Валидация deployment-а (PowerShell report)

```powershell
$report = C:\Deploy\ActivityWatch-Russian\windows\validate-deployment.ps1 `
  -ConfigPath C:\ProgramData\ActivityWatch\deployment-config.json
$report | ConvertTo-Json -Depth 12
```

---

## 4) Что должно появиться на Windows после установки

- `C:\Program Files\ActivityWatch`
- `C:\ProgramData\ActivityWatch\deployment-config.json`
- `C:\ProgramData\ActivityWatch\launch-watchers.ps1`
- `C:\ProgramData\ActivityWatch\recovery-loop.ps1`
- `C:\ProgramData\ActivityWatch\browser-domains-native-collector.ps1`
- `C:\ProgramData\ActivityWatch\web-category-rules.json`
- `C:\ProgramData\ActivityWatch\logs\`

Задачи планировщика:

- `ActivityWatch Launch [<user>]` (per-user, при логоне)
- `ActivityWatch Recovery` (system-level recovery)

---

## 5) Полная валидация потока данных

### 5.1 На Windows-хосте

Проверить процессы:

```powershell
Get-Process aw-watcher-afk,aw-watcher-window -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process | ? { $_.CommandLine -like '*browser-domains-native-collector.ps1*' } | select ProcessId,SessionId,CommandLine
```

Проверить задачи:

```powershell
Get-ScheduledTask | ? { $_.TaskName -like 'ActivityWatch*' } | select TaskName,State
```

### 5.2 На сервере ActivityWatch API

```bash
curl -sS http://127.0.0.1:5600/api/0/buckets | jq 'keys'
```

Ожидаемые bucket'ы:

- `aw-watcher-afk_<HOST>`
- `aw-watcher-window_<HOST>`
- `aw-watcher-web-<browser>_<HOST>`
- `aw-detmir-web-category_<HOST>` (категоризованный поток)

Проверка событий браузера:

```bash
curl -sS "http://127.0.0.1:5600/api/0/buckets/aw-watcher-web-edge_<HOST>/events?limit=5" | jq
```

Проверка категоризации:

```bash
curl -sS "http://127.0.0.1:5600/api/0/buckets/aw-detmir-web-category_<HOST>/events?limit=5" | jq
```

---

## 6) Сопровождение (обязательно)

### 6.1 Backup перед любыми изменениями

На Proxmox:

```bash
vzdump <CT_ID> --mode snapshot --compress zstd --storage <BACKUP_STORAGE>
```

Конфиги внутри CT:

```bash
pct exec <CT_ID> -- tar -C / -czf /root/activitywatch-config-backup.tgz \
  etc/activitywatch \
  etc/systemd/system/activitywatch-server.service \
  opt/activitywatch/webui-ru \
  opt/activitywatch/releases
```

### 6.2 Обновление сервера

1. Обновить `AW_SERVER_VERSION` и `AW_SERVER_DOWNLOAD_URL` в  
   `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env`
2. Выполнить:

```bash
/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/push-aw-artifacts.sh
pct enter <CT_ID>
bash /root/bootstrap/install_aw_server.sh
bash /root/bootstrap/apply_webui_ru_patch.sh
systemctl restart activitywatch-server.service
```

3. Повторить валидацию API/UI.

### 6.3 Rollback

RU patch rollback:

```bash
cp /opt/activitywatch/webui-ru/index.html.bak.<timestamp> /opt/activitywatch/webui-ru/index.html
systemctl restart activitywatch-server.service
```

Полный rollback:

- восстановить CT из snapshot/backup;
- проверить API и Web UI;
- проверить доступность для Windows-клиентов.

---

## 7) Безопасность

- Не хранить реальные секреты вне `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env`.
- Не открывать `5600/tcp` в интернет напрямую.
- Публиковать через VPN или reverse proxy с ограничением доступа.
- Перед изменениями всегда делать backup.

---

## 8) Короткий чек-лист ввода в эксплуатацию

1. Заполнен `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/secrets/deploy.secrets.env`.
2. Выполнен `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/create-ct.sh`.
3. Выполнен `/mnt/usb_hdd2/Projects/ActivityWatch-Russian/proxmox/push-aw-artifacts.sh`.
4. В CT выполнены `/root/bootstrap/install_aw_server.sh` и `/root/bootstrap/apply_webui_ru_patch.sh`.
5. Сервер API/порт/UI проверены.
6. На Windows выполнен `deploy-domain-users.ps1`.
7. Проверены процессы, задачи и bucket'ы.
8. Зафиксированы параметры и дата ввода.
