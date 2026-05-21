# ActivityWatch-Russian Documentation

Добро пожаловать в документацию ActivityWatch-Russian - корпоративной системы мониторинга активности на базе ActivityWatch с русификацией и DLP функциями.

## 📚 Содержание

### Архитектура
- [Обзор архитектуры](Architecture) - высокоуровневая архитектура системы
- [Компоненты системы](Components) - описание всех компонентов
- [Интерактивная карта](Interactive-Map) - визуальная карта связей
- [ИБ-профиль DLP](../dlp-security-functional-spec-ru.md) - подробное описание реализованного DLP/monitoring-контура для службы ИБ
- [Runtime status: DLP chain](../dlp-runtime-chain-status-2026-05-13.md) - фактический live-статус policy/cases/integrations/compliance
- [Runtime status: Content analysis](../dlp-content-analysis-runtime-status-2026-05-13.md) - фактический live-статус dictionary/regex/OCR/IOC
- [Hayabusa AW-rus integration](../hayabusa-aw-rus-integration-2026-05-14.md) - bounded DFIR enrichment path для incidents/cases/operator flow
- [Hayabusa operator and IB guide](../hayabusa-operator-ib-guide-2026-05-14.md) - когда запускать forensic path, где лежат артефакты и какие у него границы
- [Hayabusa Security Analytics](Hayabusa-Security-Analytics) - текущий production-контур: auto-upload, auto-case, severity scoring и Telegram alerts
- [Security analytics stack v1](../security-analytics-stack-v1.md) - целевая v1-модель без претензии на Splunk-class SIEM
- [File 1C analytics](File-1C-Analytics) - ClickHouse/Grafana/AI Investigator контур для файловой 1С

### Компоненты
- [DLP Endpoint Monitoring](DLP-Endpoint-Monitoring) - мониторинг clipboard, печати, USB
- [Browser Domains Monitoring](Browser-Domains-Monitoring) - мониторинг браузеров
- [Email Outbound Monitoring](Email-Outbound-Monitoring) - мониторинг почты
- [WebUI Русификация](WebUI-Russian-Patches) - патчи интерфейса
- [DLP Агрегация](DLP-Aggregation) - обработка DLP событий
- [Prometheus Exporter](Prometheus-Exporter) - метрики для мониторинга

### Развертывание
- [Установка на Windows](Windows-Installation) - установка коллекторов
- [Настройка сервера](Server-Setup) - настройка Linux сервера
- [Grafana + Prometheus](Monitoring-Setup) - мониторинг стек
- [Windows startup model](../windows-deploy-startup-model.md) - canonical startup model для RDP/standalone deployment

### Конфигурация
- [DLP Правила](DLP-Rules) - настройка DLP политик
- [Категоризация сайтов](Web-Categorization) - настройка категорий
- [Группы хостов](Host-Groups) - управление группами

## 🚀 Быстрый старт

### Минимальная конфигурация
```bash
# 1. Развернуть сервер
cd ansible
ansible-playbook -i inventory.ini deploy_aw_server.yml

# 2. Развернуть Windows collectors
AW_WINRM_PASSWORD='...' bash ./run_deploy_aw_windows.sh

# 3. Проверить операторский forensic path
powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-upload-hayabusa-to-aw-server.ps1 -HoursBack 6 -CaseId 30
```

### Полная конфигурация
```bash
# 1. Развертывание на Windows
.\windows\deploy-domain-users.ps1

# 2. Настройка сервера
cd ansible
ansible-playbook server-setup.yml

# 3. Запуск мониторинга стека
cd ../grafana-1c
docker-compose up -d

# 4. Для файловой 1С поднять ClickHouse/Grafana scaffold
cd ../clickhouse-1c
docker compose up -d

# 5. Агрегация DLP событий
python3 scripts/aggregate_dlp_events.py
```

## 📊 Обзор системы

ActivityWatch-Russian - это корпоративная система мониторинга активности пользователей с:

- **DLP мониторинг** - clipboard, печать, USB, браузеры, email
- **Русификация** - полный перевод интерфейса на русский
- **Аналитика** - агрегация данных и отчеты
- **Мониторинг** - Prometheus + Grafana дашборды
- **Автоматизация** - Ansible деплой на Windows и Linux
- **Security analytics** - Hayabusa, auto-case, severity scoring, Telegram alerts

## 🔗 Ссылки

- [GitHub Repository](https://github.com/igor04091968/AWatch-rus)
- [ActivityWatch Official](https://activitywatch.net/)
- [Примеры конфигураций](https://github.com/igor04091968/AWatch-rus/tree/main/grafana-1c)

## 📝 Поддержка

Для вопросов и предложений используйте [Issues](https://github.com/igor04091968/AWatch-rus/issues).
