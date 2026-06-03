# Полная инструкция по развёртыванию и поддержке ActivityWatch-Russian

Документ описывает полный цикл: Proxmox/LXC сервер, установка ActivityWatch Server, RU Web UI patch, развёртывание Windows-клиентов в другом AD-домене, валидация, сопровождение и rollback.

---

## 0) Структура проекта (полные пути)

- `<PROJECT_ROOT>/private-config/deploy.env`
- `<PROJECT_ROOT>/proxmox/create-ct.sh`
- `<PROJECT_ROOT>/proxmox/push-aw-artifacts.sh`
- `<PROJECT_ROOT>/aw-server/install_aw_server.sh`
- `<PROJECT_ROOT>/aw-server/apply_webui_ru_patch.sh`
- `<PROJECT_ROOT>/windows/deploy-single-user.ps1`
- `<PROJECT_ROOT>/windows/deploy-domain-users.ps1`
- `<PROJECT_ROOT>/windows/deploy-ensemble.ps1`
- `<PROJECT_ROOT>/windows/hardening-recovery.ps1`
- `<PROJECT_ROOT>/windows/validate-deployment.ps1`
- `<PROJECT_ROOT>/windows/browser-domains-native-collector.ps1`
- `<PROJECT_ROOT>/windows/dlp-endpoint-signals-collector.ps1`
- `<PROJECT_ROOT>/ansible/deploy_aw_server.yml`
- `<PROJECT_ROOT>/ansible/provision_proxmox_ct_and_deploy_aw.yml`
- `<PROJECT_ROOT>/ansible/provision_proxmox_ct_matrix_and_deploy_aw.yml`
- `<PROJECT_ROOT>/ansible/deploy_aw_windows.yml`

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
cp <PROJECT_ROOT>/private-config/deploy.env.example \
   <PROJECT_ROOT>/private-config/deploy.env
```

Заполните в файле `<PROJECT_ROOT>/private-config/deploy.env`:

- все `CT_*` параметры контейнера;
- все `AW_SERVER_*` параметры сервера;
- `CT_PASSWORD` (реальный пароль).

Важно: этот файл подхватывается автоматически скриптами Proxmox.

---

## 2) Развёртывание сервера в Proxmox

### 2.0 Ansible full-stack (создание CT + установка AW)

Подготовьте:

- `<PROJECT_ROOT>/ansible/inventory.ini`
- `<PROJECT_ROOT>/ansible/group_vars/all.yml`
- `<PROJECT_ROOT>/ansible/group_vars/proxmox.yml`

Запуск:

```bash
cd <PROJECT_ROOT>/ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_and_deploy_aw.yml
```

Этот сценарий полностью закрывает:

- создание CT в Proxmox;
- bootstrap пакетов в CT;
- установку ActivityWatch Server;
- применение RU Web UI patch;
- проверку API.

Для массового режима (несколько CT):

```bash
cd <PROJECT_ROOT>/ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_matrix_and_deploy_aw.yml
```

### 2.1 Создать LXC контейнер

На узле Proxmox:

```bash
cd <PROJECT_ROOT>
<PROJECT_ROOT>/proxmox/create-ct.sh
```

По умолчанию читается:

- `<PROJECT_ROOT>/private-config/deploy.env`

При необходимости можно передать другой путь:

```bash
<PROJECT_ROOT>/proxmox/create-ct.sh /absolute/path/to/deploy.env
```

### 2.2 Загрузить bootstrap-артефакты и env внутрь CT

```bash
cd <PROJECT_ROOT>
<PROJECT_ROOT>/proxmox/push-aw-artifacts.sh
```

Скрипт загружает в CT:

- `<CT_BOOTSTRAP_DIR>/install_aw_server.sh`
- `<CT_BOOTSTRAP_DIR>/apply_webui_ru_patch.sh`
- `<CT_BOOTSTRAP_DIR>/activitywatch-server.service`
- `<CT_BOOTSTRAP_DIR>/aw-ru-patch.js`
- `<CT_BOOTSTRAP_DIR>/aw-sw-cleanup.js`
- `/etc/activitywatch/aw-server.env` (из `AW_SERVER_*`)

### 2.3 Установить ActivityWatch Server внутри CT

```bash
pct enter <CT_ID>
bash <CT_BOOTSTRAP_DIR>/install_aw_server.sh
```

### 2.4 Применить RU patch Web UI

```bash
bash <CT_BOOTSTRAP_DIR>/apply_webui_ru_patch.sh
systemctl restart activitywatch-server.service
```

После применения патча доступны:

- верхнее меню `DLP` в Web UI;
- DLP-страница bucket `aw-dlp-endpoint-signals_<HOST>`;
- встроенный центр `DLP review и правила`;
- служебные buckets `aw-dlp-review_<HOST>` и `aw-dlp-rules_<HOST>`.

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

Дополнительно после первого входа в Web UI:

- `#/home` должен показывать один корректный пункт `DLP`;
- `#/buckets/aw-dlp-endpoint-signals_<HOST>` должен открываться без ошибок;
- сохранение review/rule должно создавать buckets `aw-dlp-review_<HOST>` и `aw-dlp-rules_<HOST>`.

---

## 3) Развёртывание Windows-клиентов (другой AD-домен)

### 3.1 Подготовка на Windows-хосте

Скопируйте каталог:

- `<PROJECT_ROOT>/windows`

например в:

- `C:\Program Files\AWatch-rus\windows`

Откройте **elevated PowerShell**:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope Process
```

### 3.2 Массовое доменное развёртывание (рекомендуется)

Если текущий production ещё работает в старых каталогах
`C:\Program Files\ActivityWatch-Phase2` и `C:\ProgramData\ActivityWatch-Phase2`,
сначала выполните безопасную миграцию:

```powershell
C:\Program Files\AWatch-rus\windows\migrate-awatch-rus-paths.ps1 -WhatIf
C:\Program Files\AWatch-rus\windows\migrate-awatch-rus-paths.ps1
```

Скрипт остановит `ActivityWatch Recovery`/`ActivityWatch Launch *`, создаст backup в
`C:\ProgramData\AWatch-rus\migration-backups\...`, перенесёт файлы в единые пути,
пересоздаст `deployment-config.json`/scheduled tasks и запустит validation.

Пример со списком пользователей:

```powershell
C:\Program Files\AWatch-rus\windows\deploy-domain-users.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -Domain CONTOSO `
  -UserListPath C:\Deploy\aw-users.txt `
  -CustomRulesPath C:\Program Files\AWatch-rus\windows\web-category-rules.example.json
```

Поддерживаемые варианты:

- `-Users user01,user02`
- `-Users 'CONTOSO\user01','CONTOSO\user02'`
- `-UserListPath <txt|csv>`

### 3.2.1 Ensemble orchestration (рекомендуется для production)

```powershell
C:\Program Files\AWatch-rus\windows\deploy-ensemble.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -Domain CONTOSO `
  -Users user1,user2,user3,user4,user5 `
  -ValidateAfterDeploy
```

Отчёт сохраняется в:

- `C:\ProgramData\AWatch-rus\ensemble-report-YYYYMMDD-HHMMSS.json`

### 3.3 Single-user развёртывание

```powershell
C:\Program Files\AWatch-rus\windows\deploy-single-user.ps1 `
  -ServerHost aw.example.local `
  -ServerPort 5600 `
  -TargetUser 'CONTOSO\user01' `
  -CustomRulesPath C:\Program Files\AWatch-rus\windows\web-category-rules.example.json
```

### 3.4 Recovery / hardening

```powershell
C:\Program Files\AWatch-rus\windows\hardening-recovery.ps1 `
  -ConfigPath C:\ProgramData\AWatch-rus\deployment-config.json
```

### 3.5 Валидация deployment-а (PowerShell report)

```powershell
$report = C:\Program Files\AWatch-rus\windows\validate-deployment.ps1 `
  -ConfigPath C:\ProgramData\AWatch-rus\deployment-config.json
$report | ConvertTo-Json -Depth 12
```

---

## 4) Что должно появиться на Windows после установки

- `C:\Program Files\AWatch-rus\bin`
- `C:\ProgramData\AWatch-rus\deployment-config.json`
- `C:\ProgramData\AWatch-rus\launch-watchers.ps1`
- `C:\ProgramData\AWatch-rus\recovery-loop.ps1`
- `C:\ProgramData\AWatch-rus\browser-domains-native-collector.ps1`
- `C:\ProgramData\AWatch-rus\web-category-rules.json`
- `C:\ProgramData\AWatch-rus\logs\`

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
- `aw-dlp-endpoint-signals_<HOST>` (endpoint сигналы)
- `aw-dlp-review_<HOST>` (ручная классификация через UI)
- `aw-dlp-rules_<HOST>` (suppress/rule записи через UI)

Проверка событий браузера:

```bash
curl -sS "http://127.0.0.1:5600/api/0/buckets/aw-watcher-web-edge_<HOST>/events?limit=5" | jq
```

Проверка категоризации:

```bash
curl -sS "http://127.0.0.1:5600/api/0/buckets/aw-detmir-web-category_<HOST>/events?limit=5" | jq
```

Проверка DLP review/rules:

```bash
curl -sS "http://127.0.0.1:5600/api/0/buckets/aw-dlp-review_<HOST>/events?limit=20" | jq
curl -sS "http://127.0.0.1:5600/api/0/buckets/aw-dlp-rules_<HOST>/events?limit=20" | jq
```

Ожидаемые поля review:

- `reviewId`
- `signalType`
- `verdict`
- `category`
- `comment`
- `archived`

Ожидаемые поля rules:

- `ruleId`
- `signalType`
- `match`
- `category`
- `comment`
- `enabled`

---

## 6) Сопровождение (обязательно)

### 6.1 Backup перед любыми изменениями

На Proxmox:

```bash
vzdump <CT_ID> --mode snapshot --compress zstd --storage <BACKUP_STORAGE>
```

Конфиги внутри CT:

```bash
pct exec <CT_ID> -- tar -C / -czf <PRIVATE_BACKUP_DIR>/activitywatch-config-backup.tgz \
  etc/activitywatch \
  etc/systemd/system/activitywatch-server.service \
  opt/activitywatch/webui-ru \
  opt/activitywatch/releases
```

### 6.2 Обновление сервера

1. Обновить `AW_SERVER_VERSION` и `AW_SERVER_DOWNLOAD_URL` в  
   `<PROJECT_ROOT>/private-config/deploy.env`
2. Выполнить:

```bash
<PROJECT_ROOT>/proxmox/push-aw-artifacts.sh
pct enter <CT_ID>
bash <CT_BOOTSTRAP_DIR>/install_aw_server.sh
bash <CT_BOOTSTRAP_DIR>/apply_webui_ru_patch.sh
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

- Не хранить реальные приватные параметры вне `<PROJECT_ROOT>/private-config/deploy.env`.
- Не открывать `5600/tcp` в интернет напрямую.
- Публиковать через VPN или reverse proxy с ограничением доступа.
- Перед изменениями всегда делать backup.

---

## 8) Короткий чек-лист ввода в эксплуатацию

1. Заполнен `<PROJECT_ROOT>/private-config/deploy.env`.
2. Выполнен `<PROJECT_ROOT>/proxmox/create-ct.sh`.
3. Выполнен `<PROJECT_ROOT>/proxmox/push-aw-artifacts.sh`.
4. В CT выполнены `<CT_BOOTSTRAP_DIR>/install_aw_server.sh` и `<CT_BOOTSTRAP_DIR>/apply_webui_ru_patch.sh`.
5. Сервер API/порт/UI проверены.
6. На Windows выполнен `deploy-domain-users.ps1`.
7. Проверены процессы, задачи и bucket'ы.
8. Зафиксированы параметры и дата ввода.
