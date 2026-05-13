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
# 1. Установка на Windows workstation
.\windows\deploy-domain-users.ps1

# 2. Запуск ActivityWatch Server
./aw-server/aw-server

# 3. Применение RU патчей
node aw-server/aw-ru-patch.js
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

# 4. Агрегация DLP событий
python3 scripts/aggregate_dlp_events.py
```

## 📊 Обзор системы

ActivityWatch-Russian - это корпоративная система мониторинга активности пользователей с:

- **DLP мониторинг** - clipboard, печать, USB, браузеры, email
- **Русификация** - полный перевод интерфейса на русский
- **Аналитика** - агрегация данных и отчеты
- **Мониторинг** - Prometheus + Grafana дашборды
- **Автоматизация** - Ansible деплой на Windows и Linux

## 🔗 Ссылки

- [GitHub Repository](https://github.com/igor04091968/AWatch-rus)
- [ActivityWatch Official](https://activitywatch.net/)
- [Примеры конфигураций](https://github.com/igor04091968/AWatch-rus/tree/main/grafana-1c)

## 📝 Поддержка

Для вопросов и предложений используйте [Issues](https://github.com/igor04091968/AWatch-rus/issues).
