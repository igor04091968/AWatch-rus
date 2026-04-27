# pfSense -> ActivityWatch

Для `pfSense` используется внешний poller на `Debian/Ubuntu` utility VM. Сам firewall не трогаем Windows-агентами и не превращаем в desktop-client.

## Схема

- `pfSense` отвечает по API.
- `Debian/Ubuntu` poller VM опрашивает его с интервалом.
- poller отправляет события в общий `AW server`.

## Bucket'ы

Базовая конфигурация пишет:

- `aw-pfsense-health_<HOST>`
- `aw-pfsense-interfaces_<HOST>`
- `aw-pfsense-gateways_<HOST>`

## Файлы

- [pfsense-aw-poller.py](/home/igor/tmp/AWatch-rus/pfsense/pfsense-aw-poller.py)
- [pfsense-aw-poller.service](/home/igor/tmp/AWatch-rus/pfsense/pfsense-aw-poller.service)
- [pfsense-aw-poller.example.json](/home/igor/tmp/AWatch-rus/pfsense/pfsense-aw-poller.example.json)
- [deploy_aw_pfsense_poller.yml](/home/igor/tmp/AWatch-rus/ansible/deploy_aw_pfsense_poller.yml)
- [pfsense-poller.example.yml](/home/igor/tmp/AWatch-rus/ansible/group_vars/pfsense-poller.example.yml)

## Ручной запуск

```bash
sudo install -d /opt/aw-pfsense /etc/aw-pfsense
sudo cp /path/to/pfsense-aw-poller.py /opt/aw-pfsense/
sudo chmod 0755 /opt/aw-pfsense/pfsense-aw-poller.py
sudo cp /path/to/poller.json /etc/aw-pfsense/poller.json
python3 /opt/aw-pfsense/pfsense-aw-poller.py --config /etc/aw-pfsense/poller.json --once
```

## Через Ansible

```bash
cd /home/igor/tmp/AWatch-rus/ansible
cp group_vars/pfsense-poller.example.yml group_vars/pfsense-poller.yml
ansible-playbook -i inventory.ini deploy_aw_pfsense_poller.yml
```

Inventory:

```ini
[aw_pfsense_pollers]
aw-poller-01 ansible_host=10.10.10.50
```

## AW Web

На `#/home` добавлен отдельный блок:

- `Windows RDP`
- `Virtual servers + Proxmox`

Для `pfSense` задавай hostname вида `PFSENSE-EDGE01`, тогда он попадёт во вторую группу.
