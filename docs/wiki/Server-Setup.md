# Настройка сервера

Настройка Linux сервера для ActivityWatch-Russian.

## Требования

- Linux (Ubuntu 20.04+, Debian 11+, CentOS 8+)
- Python 3.8+
- PostgreSQL 12+
- 4GB+ RAM
- 50GB+ disk space

## Установка через Ansible

```bash
cd ansible
ansible-playbook server-setup.yml
```

## Ручная установка

### 1. Установка ActivityWatch Server

```bash
# Скачайте binaries
wget https://github.com/ActivityWatch/activitywatch/releases/download/v0.13.1/activitywatch-v0.13.1-linux-x86_64.zip

# Распакуйте
unzip activitywatch-v0.13.1-linux-x86_64.zip
cd activitywatch-v0.13.1-linux-x86_64

# Запустите сервер
./aw-server
```

### 2. Установка PostgreSQL

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install postgresql postgresql-contrib

# CentOS/RHEL
sudo yum install postgresql-server postgresql-contrib
sudo postgresql-setup initdb
sudo systemctl start postgresql
sudo systemctl enable postgresql

# Создайте базу данных
sudo -u postgres psql
CREATE DATABASE activitywatch;
CREATE USER aw WITH PASSWORD 'your_password';
GRANT ALL PRIVILEGES ON DATABASE activitywatch TO aw;
\q
```

### 3. Применение RU патчей

```bash
# Скопируйте патчи
cp aw-server/aw-ru-patch.js /path/to/aw-server/
cp aw-server/aw-sw-cleanup.js /path/to/aw-server/

# Примените патчи
cd /path/to/aw-server/
node aw-ru-patch.js
```

### 4. Настройка systemd service

```bash
sudo nano /etc/systemd/system/aw-server.service
```

```ini
[Unit]
Description=ActivityWatch Server
After=network.target

[Service]
Type=simple
User=aw
WorkingDirectory=/opt/activitywatch
ExecStart=/opt/activitywatch/aw-server
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable aw-server
sudo systemctl start aw-server
```

## Конфигурация

### ActivityWatch Config

```toml
# /opt/activitywatch/server-config.toml
[server]
host = "0.0.0.0"
port = 5600

[logging]
level = "INFO"
file = "/var/log/activitywatch/aw-server.log"
```

### PostgreSQL Config

```bash
# /etc/postgresql/12/main/postgresql.conf
listen_addresses = '*'
max_connections = 100
shared_buffers = 256MB
```

## Установка Python зависимостей

```bash
# Создайте virtual environment
python3 -m venv /opt/aw-venv
source /opt/aw-venv/bin/activate

# Установите зависимости
pip install psycopg2-binary requests prometheus_client
```

## Настройка скриптов агрегации

```bash
# Скопируйте скрипты
cp scripts/aggregate_dlp_events.py /opt/activitywatch/

# Настройте cron job
crontab -e
```

```cron
*/5 * * * * /opt/aw-venv/bin/python /opt/activitywatch/aggregate_dlp_events.py
```

## Firewall

```bash
# Ubuntu/Debian (ufw)
sudo ufw allow 5600/tcp
sudo ufw allow 5432/tcp
sudo ufw enable

# CentOS/RHEL (firewalld)
sudo firewall-cmd --permanent --add-port=5600/tcp
sudo firewall-cmd --permanent --add-port=5432/tcp
sudo firewall-cmd --reload
```

## Проверка

```bash
# Проверка ActivityWatch Server
curl http://localhost:5600/api/0/buckets

# Проверка PostgreSQL
sudo -u postgres psql -c "SELECT version();"

# Проверка логов
sudo journalctl -u aw-server -f
```

## Мониторинг

```bash
# Проверка статуса
sudo systemctl status aw-server

# Проверка ресурсов
htop

# Проверка диска
df -h

# Проверка PostgreSQL
sudo -u postgres psql -c "SELECT pg_stat_database.datname, pg_stat_database.numbackends FROM pg_stat_database;"
```

## Backup

```bash
# Backup PostgreSQL
sudo -u postgres pg_dump activitywatch > backup_$(date +%Y%m%d).sql

# Backup ActivityWatch data
sudo tar -czf aw-backup_$(date +%Y%m%d).tar.gz /opt/activitywatch/data

# Restore
sudo -u postgres psql activitywatch < backup_20240101.sql
```

## Обновление

```bash
# Остановите сервер
sudo systemctl stop aw-server

# Backup
sudo cp -r /opt/activitywatch /opt/activitywatch.backup

# Обновите binaries
wget https://github.com/ActivityWatch/activitywatch/releases/download/v0.14.0/activitywatch-v0.14.0-linux-x86_64.zip
unzip activitywatch-v0.14.0-linux-x86_64.zip -d /opt/activitywatch/

# Запустите сервер
sudo systemctl start aw-server
```

## Устранение проблем

### Server не запускается
```bash
# Проверьте логи
sudo journalctl -u aw-server -n 50

# Проверьте порт
sudo netstat -tlnp | grep 5600

# Проверьте права
sudo ls -la /opt/activitywatch
```

### PostgreSQL connection refused
```bash
# Проверьте статус
sudo systemctl status postgresql

# Проверьте конфиг
sudo cat /etc/postgresql/12/main/postgresql.conf | grep listen_addresses

# Проверьте firewall
sudo ufw status
```

## Подробнее

- [Мониторинг стек](Monitoring-Setup)
- [DLP Агрегация](DLP-Aggregation)
