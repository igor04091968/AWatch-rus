# Ansible ensemble for AWatch-rus

Эта директория содержит минимальный Ansible-ensemble для повторяемого развёртывания ActivityWatch Server в Debian/LXC.

## Файлы

- `/home/igor/tmp/AWatch-rus/ansible/deploy_aw_server.yml` — основной playbook.
- `/home/igor/tmp/AWatch-rus/ansible/inventory.example.ini` — шаблон inventory.
- `/home/igor/tmp/AWatch-rus/ansible/group_vars/all.example.yml` — шаблон переменных.

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

## Результат

- Установлен ActivityWatch Server.
- Создан systemd-unit `activitywatch-server.service`.
- Установлен RU Web UI patch.
- Выполнена валидация API `http://127.0.0.1:5600/api/0/info`.
