# Ansible ensemble for AWatch-rus

Эта директория содержит Ansible-ensemble для двух сценариев:

- деплой на уже существующий Debian host/CT;
- полный цикл с нуля в Proxmox: создание CT + bootstrap + установка ActivityWatch + RU patch.

## Файлы

- `/home/igor/tmp/AWatch-rus/ansible/deploy_aw_server.yml` — основной playbook.
- `/home/igor/tmp/AWatch-rus/ansible/provision_proxmox_ct_and_deploy_aw.yml` — full-stack playbook для Proxmox.
- `/home/igor/tmp/AWatch-rus/ansible/inventory.example.ini` — шаблон inventory.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/all.example.yml` — шаблон переменных.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/proxmox.example.yml` — шаблон переменных CT в Proxmox.

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

## Результат

- Установлен ActivityWatch Server.
- Создан systemd-unit `activitywatch-server.service`.
- Установлен RU Web UI patch.
- Выполнена валидация API `http://127.0.0.1:5600/api/0/info`.
- Для full-stack сценария CT создаётся автоматически через `pct create`.
