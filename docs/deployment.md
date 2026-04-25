# Deployment

## 0. Ansible full-stack вариант (рекомендуется)

```sh
cd /home/igor/tmp/AWatch-rus/ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_and_deploy_aw.yml
```

Этот playbook создаёт CT в Proxmox и полностью разворачивает ActivityWatch внутри контейнера.

Массовый вариант по матрице CT:

```sh
cd /home/igor/tmp/AWatch-rus/ansible
ansible-playbook -i inventory.ini provision_proxmox_ct_matrix_and_deploy_aw.yml
```

## 1. Подготовить env-файлы

На рабочей машине оператора:

```sh
cp proxmox/ct-vars.example.env /root/activitywatch-ct.env
cp aw-server/aw-server.env.example /root/activitywatch-aw.env
```

Заполнить оба файла реальными значениями вне git.

## 2. Создать CT на Proxmox

На узле Proxmox:

```sh
cd /path/to/ActivityWatch-Russian
./proxmox/create-ct.sh /root/activitywatch-ct.env
```

Скрипт:

- валидирует обязательные переменные;
- создаёт Debian 12 CT;
- запускает контейнер;
- выполняет минимальный bootstrap пакетов;
- готовит `/root/bootstrap` для дальнейшей загрузки артефактов.

## 3. Загрузить артефакты в CT

На узле Proxmox:

```sh
./proxmox/push-aw-artifacts.sh /root/activitywatch-ct.env
pct push <CT_ID> /root/activitywatch-aw.env /etc/activitywatch/aw-server.env
```

В CT будут загружены:

- `install_aw_server.sh`
- `apply_webui_ru_patch.sh`
- `activitywatch-server.service`
- `aw-ru-patch.js`
- `aw-sw-cleanup.js`

## 4. Установить ActivityWatch Server

Внутри CT:

```sh
pct enter <CT_ID>
bash /root/bootstrap/install_aw_server.sh
```

Скрипт установки:

- ставит `curl`, `unzip`, `jq`, `ca-certificates`;
- создаёт пользователя `activitywatch`;
- скачивает release `aw-server-rust`;
- раскладывает бинарник по версиям;
- создаёт активный symlink;
- копирует systemd unit;
- включает и запускает сервис.

## 5. Применить RU patch для Web UI

Внутри CT:

```sh
bash /root/bootstrap/apply_webui_ru_patch.sh
systemctl restart activitywatch-server.service
```

Патч:

- копирует `aw-ru-patch.js` и `aw-sw-cleanup.js`;
- делает backup `index.html`;
- добавляет include в `index.html`;
- заменяет `service-worker.js` cleanup-версией для сброса старого cache.

## 6. Проверить сервис

```sh
systemctl status activitywatch-server.service --no-pager
curl -fsS http://127.0.0.1:5600/api/0/info
ss -ltnp | grep 5600
grep -n 'aw-ru-patch\|aw-sw-cleanup' /opt/activitywatch/webui-ru/index.html
```

Ожидаемо:

- сервис `active (running)`;
- API отвечает;
- порт слушается;
- в `index.html` есть оба include.

## 7. После публикации

Проверить извне:

- открывается `/`;
- API доступен по ожидаемому URL;
- после hard refresh видна русификация;
- reverse proxy не кэширует старую статику.

## 8. Что зафиксировать после ввода

- дата ввода;
- узел Proxmox;
- `CT_ID`;
- IP/FQDN;
- версия `aw-server-rust`;
- место хранения backup;
- фактическая схема публикации.
