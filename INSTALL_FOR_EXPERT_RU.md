# Установка экземпляра для эксперта

Этот документ дает короткий воспроизводимый путь проверки экземпляра без
привязки к личному стенду разработчика.

## 1. Подготовка

1. Склонировать репозиторий.
2. Создать приватную конфигурацию:

   ```bash
   cp private-config/deploy.env.example private-config/deploy.env
   ```

3. Создать локальный Ansible inventory на основе:

   ```bash
   cp ansible/inventory.example.ini ansible/inventory.ini
   ```

4. Заполнить адреса, учетные данные и токены конкретного тестового стенда.

## 2. Сборка

```bash
cd adk-rust
cargo build --release --workspace
```

## 3. Проверки до установки

```bash
scripts/quality-gate.sh
ansible-playbook --syntax-check -i ansible/inventory.ini ansible/deploy_aw_server.yml
```

## 4. Установка

Базовый серверный путь описан в `docs/INSTALL_RU.md`. Конкретный playbook
выбирается по проверяемой схеме: серверный runtime, Windows collectors,
Grafana dashboards или портал оператора.

## 5. Smoke-проверка

После установки:

```bash
detmir-check
detmir-status
```

Ожидаемый результат: статус `OK`, отсутствуют критичные service failures и
stale/dead buckets для обязательных источников.
