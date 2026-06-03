# Установка экземпляра для эксперта

Документ описывает воспроизводимый путь: чистая VM -> установка -> проверка ->
ожидаемый результат. Он не использует адреса, домены и учетные данные
боевого стенда.

## 1. Цель проверки

Эксперт должен получить развернутый экземпляр DetMir/AWatch-rus, убедиться,
что ПО собирается из исходного кода, устанавливается на чистую Linux VM,
запускает базовые сервисы и проходит smoke-проверки без приватной
инфраструктуры правообладателя.

## 2. Минимальный стенд

Рекомендуемая чистая VM:

- Debian 12 или Ubuntu Server 24.04 LTS;
- 2 vCPU;
- 4 GB RAM для минимальной проверки, 8 GB RAM для проверки Grafana/Prometheus;
- 30 GB свободного диска;
- доступ в интернет для установки пакетов и Rust crates;
- пользователь с правами `sudo`;
- корректные DNS, NTP и системное время.

Опционально для полной проверки контура:

- отдельная Windows VM или физический Windows host для collector toolkit;
- отдельный host или VM для Grafana/Prometheus, если они не ставятся на ту же
  Linux VM;
- закрытая тестовая сеть с адресами, заданными в `ansible/inventory.ini`.

## 3. Подготовка чистой VM

Войти на VM под пользователем с `sudo` и установить базовые инструменты:

```bash
sudo apt-get update
sudo apt-get install -y \
  ca-certificates curl git jq unzip tar xz-utils \
  build-essential pkg-config libssl-dev sqlite3 \
  python3 python3-venv python3-pip ansible
```

Проверить версии:

```bash
git --version
python3 --version
ansible --version
```

Ожидаемый результат:

- команды завершаются без ошибок;
- установлен Git, Python 3 и Ansible;
- VM имеет доступ к пакетным репозиториям.

## 4. Установка Rust toolchain

Установить стабильный Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
rustup default stable
rustc --version
cargo --version
```

Ожидаемый результат:

- `rustc --version` и `cargo --version` выводят стабильную версию;
- `$HOME/.cargo/bin` доступен в текущей shell-сессии.

## 5. Получение исходного кода

Склонировать репозиторий и перейти в рабочую директорию:

```bash
git clone https://github.com/igor04091968/AWatch-rus.git
cd AWatch-rus
git rev-parse --short HEAD
```

Ожидаемый результат:

- репозиторий склонирован;
- команда `git rev-parse --short HEAD` выводит commit id проверяемой версии.

## 6. Проверка публичной гигиены поставки

Перед установкой выполнить быстрый контроль отсутствия приватных маркеров в
публичных документах:

```bash
PRIVATE_MARKERS_REGEX='<PRIVATE_HOSTNAME>|<PRIVATE_PUBLIC_DOMAIN>|<LOCAL_OPERATOR_HOME>|<ROOT_PRIVATE_PATH>'
git grep -n -E "$PRIVATE_MARKERS_REGEX" -- \
  README.md docs REGISTER_RU_SOFTWARE.md PRODUCT_DESCRIPTION_RU.md \
  SECURITY_OVERVIEW_RU.md adk-rust/RUNBOOK.md || true
```

Ожидаемый результат:

- команда не должна находить реальные имена хостов, личные домены и личные
  пути оператора в публичных документах;
- допустимы только нейтральные placeholders вроде `<AW_SERVER_HOST>`,
  `<PUBLIC_GATEWAY_FQDN>`, `HOST-EXAMPLE`.

## 7. Подготовка локальной конфигурации

Создать приватные файлы из примеров:

```bash
cp private-config/deploy.env.example private-config/deploy.env
cp ansible/inventory.example.ini ansible/inventory.ini
```

Заполнить в `private-config/deploy.env` и `ansible/inventory.ini` только
тестовые значения:

- адрес Linux VM;
- адрес Windows host, если проверяются Windows collectors;
- адрес Grafana/Prometheus, если они вынесены на отдельную VM;
- тестовые учетные данные;
- тестовые токены Telegram/Webhook, если проверяются уведомления.

Ожидаемый результат:

- приватная конфигурация существует локально;
- приватные файлы не попадают в Git благодаря `.gitignore`;
- в публичных файлах не появляются реальные пароли, IP и домены.

## 8. Сборка Rust-компонентов

Собрать workspace:

```bash
cd adk-rust
cargo build --release --workspace
cd ..
```

Ожидаемый результат:

- сборка завершается кодом `0`;
- release-бинарники появляются в `adk-rust/target/release/`;
- отсутствуют ошибки компиляции Rust-компонентов.

## 9. Локальные тесты до установки

Запустить базовые проверки исходников:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo test --workspace
cd ..
```

Проверить Ansible syntax:

```bash
ansible-playbook --syntax-check -i ansible/inventory.ini ansible/deploy_aw_server.yml
```

Ожидаемый результат:

- `cargo fmt` не сообщает diff;
- `cargo test --workspace` завершается успешно;
- Ansible syntax-check не находит ошибок YAML/playbook.

## 10. Установка минимального серверного экземпляра

Для чистой экспертной VM использовать inventory, где целевой Linux host
указывает на эту же VM или на отдельный тестовый сервер.

Пример команды:

```bash
ansible-playbook -i ansible/inventory.ini ansible/deploy_aw_server.yml
```

Если playbook требует переменные окружения или группы hosts, задать их в
локальном inventory, не меняя публичные файлы репозитория.

Ожидаемый результат:

- playbook завершается без failed tasks;
- systemd units установлены на тестовый Linux host;
- ActivityWatch server и DetMir helper-компоненты доступны локально на
  заданных портах;
- конфигурация не содержит боевых адресов правообладателя.

## 11. Проверка сервисов после установки

На тестовом Linux host выполнить:

```bash
systemctl --failed --no-pager
systemctl status activitywatch-server --no-pager
```

Если установлены Rust helper-бинарники:

```bash
detmir-check --json
detmir-status --json
```

Ожидаемый результат:

- `systemctl --failed` не показывает критичных failed units DetMir/AWatch-rus;
- `activitywatch-server` находится в состоянии `active`;
- `detmir-check --json` возвращает машинно-читаемый статус без критичных
  ошибок;
- `detmir-status --json` показывает агрегированный статус установленного
  экземпляра.

## 12. Проверка HTTP API

Проверить ActivityWatch API:

```bash
curl -fsS http://127.0.0.1:5600/api/0/info | jq .
curl -fsS http://127.0.0.1:5600/api/0/buckets/ | jq 'keys | length'
```

Ожидаемый результат:

- `/api/0/info` возвращает JSON;
- `/api/0/buckets/` возвращает JSON-объект;
- ошибки `connection refused`, `403`, `500` отсутствуют.

## 13. Проверка Grafana/Prometheus

Если в экспертном стенде установлены Grafana/Prometheus:

```bash
curl -fsS http://127.0.0.1:3000/api/health | jq .
curl -fsS http://127.0.0.1:9090/-/ready
```

Ожидаемый результат:

- Grafana health API возвращает статус `ok` или `database: ok`;
- Prometheus ready endpoint возвращает успешный HTTP status;
- DetMir dashboards импортированы или доступны через documented provisioning
  path.

## 14. Проверка Windows collectors

Этот шаг нужен только для полной проверки контура.

На Windows host выполнить PowerShell deployment из `windows/` или Ansible
playbook для Windows collectors, используя тестовый domain/hostname:

```powershell
Get-ScheduledTask | Where-Object TaskName -like 'ActivityWatch*'
```

На Linux server проверить появление buckets:

```bash
curl -fsS http://127.0.0.1:5600/api/0/buckets/ | jq 'keys[]' | sort
```

Ожидаемый результат:

- Windows tasks созданы с тестовыми именами host/user;
- ActivityWatch buckets получают события от Windows collectors;
- в bucket names нет приватных имен боевого стенда.

## 15. Smoke-проверка портала оператора

Если установлен portal runtime:

```bash
curl -fsS http://127.0.0.1:8080/health
```

Ожидаемый результат:

- health endpoint возвращает успешный HTTP status;
- карточки портала открываются по локальным адресам стенда;
- ссылки на Grafana/ActivityWatch/incident evidence используют значения из
  конфигурации, а не захардкоженные адреса.

## 16. Итоговый критерий приемки

Экземпляр считается установленным корректно, если:

- исходный код собирается командой `cargo build --release --workspace`;
- локальные Rust tests проходят;
- Ansible syntax-check проходит;
- серверный playbook завершается без failed tasks;
- ActivityWatch API отвечает JSON;
- `systemctl --failed` не показывает критичных failures;
- `detmir-check` и `detmir-status` отрабатывают;
- публичные документы не содержат приватных IP, доменов, hostnames и путей;
- third-party license inventory и release/SBOM checklist заполнены.

## 17. Сбор диагностического пакета для эксперта

После проверки сохранить артефакты:

```bash
mkdir -p expert-check-output
git rev-parse HEAD > expert-check-output/commit.txt
systemctl --failed --no-pager > expert-check-output/systemd-failed.txt
curl -fsS http://127.0.0.1:5600/api/0/info > expert-check-output/aw-info.json
detmir-check --json > expert-check-output/detmir-check.json || true
detmir-status --json > expert-check-output/detmir-status.json || true
```

Ожидаемый результат:

- в `expert-check-output/` есть commit id, systemd summary и JSON-проверки;
- пакет не содержит паролей, токенов и приватных доменов.

## 18. Очистка тестового экземпляра

Для удаления тестовой VM достаточно уничтожить саму VM. Если нужно очистить
только сервисы внутри VM, сначала сохранить diagnostic output, затем остановить
установленные units:

```bash
sudo systemctl stop activitywatch-server || true
sudo systemctl list-units 'aw-*' 'detmir-*' --no-pager
```

Ожидаемый результат:

- экспертский стенд можно воспроизвести заново из исходников и локальной
  конфигурации;
- очистка не требует доступа к боевому стенду правообладателя.
