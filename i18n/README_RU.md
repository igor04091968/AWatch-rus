# Интернационализация (i18n) в AWatch-rus

## Обзор

Модуль интернационализации обеспечивает поддержку многоязычного интерфейса для PowerShell-скриптов проекта AWatch-rus.

## Возможности

- **JSON-based каталоги сообщений** — удобное хранение и редактирование переводов
- **Автоматический fallback** — при отсутствии перевода используется резервный язык
- **Параметризованные сообщения** — поддержка форматирования с плейсхолдерами `{0}`, `{1}`, etc.
- **Автоверсионизация** — семантическое версионирование каталогов переводов
- **Валидация** — проверка структуры и консистентности переводов
- **Анализ покрытия** — отчет о полноте переводов по языкам

## Структура

```
i18n/
├── en-US.json              # Reference locale (English)
├── ru-RU.json              # Russian translation
├── package.json            # Node.js scripts configuration
└── scripts/
    ├── validate-locales.js # Валидация JSON-файлов
    ├── check-coverage.js   # Анализ покрытия переводов
    └── bump-version.js     # Автоверсионирование
```

## Быстрый старт

### 1. Инициализация локали в PowerShell скрипте

```powershell
# Импортируйте модуль i18n
Import-Module "$PSScriptRoot\ActivityWatch.Windows.I18n.psm1" -Force

# Инициализируйте русскую локаль с английским fallback
Initialize-Locale -Culture "ru-RU" -FallbackCulture "en-US"

# Или автодетект культуры системы
Initialize-Locale -AutoDetect
```

### 2. Использование локализованных строк

```powershell
# Простое сообщение
$message = Get-LocalizedString -Key "errors.admin_required"

# Сообщение с параметрами
$message = Get-LocalizedString -Key "errors.binary_missing" -FormatArgs @("aw-watcher-afk.exe")

# Вывод информации
Get-LocalizedInfo -Key "starting_deployment" -FormatArgs @("v0.13.2")

# Вывод предупреждения
Get-LocalizedWarning -Key "insecure_connection"

# Создание ошибки
$errorRecord = Get-LocalizedError -Key "config_not_found" -FormatArgs @($configPath)
throw $errorRecord

# Статусы
$status = Get-LocalizedStatus -Key "installation_success"

# Подтверждение от пользователя
if (Read-LocalizedConfirm -PromptKey "confirm_install" -FormatArgs @($userCount)) {
    # Продолжить установку
}
```

### 3. Категории сообщений

| Категория | Префикс ключа | Пример использования |
|-----------|---------------|---------------------|
| Errors | `errors.*` | Сообщения об ошибках, исключения |
| Info | `info.*` | Информационные сообщения |
| Warnings | `warnings.*` | Предупреждения |
| Prompts | `prompts.*` | Запросы к пользователю |
| Status | `status.*` | Статусы операций |
| Choices | `choices.*` | Варианты выбора |

## Формат JSON каталога

```json
{
  "version": "1.0.0",
  "language": "ru",
  "fallback": "en",
  "messages": {
    "errors.admin_required": "Запустите этот скрипт из сеанса PowerShell с правами администратора.",
    "errors.binary_missing": "Отсутствует требуемый двоичный файл ActivityWatch: {0}",
    "info.starting_deployment": "Начало развертывания ActivityWatch версии {0}...",
    "prompts.confirm_install": "Вы уверены, что хотите установить ActivityWatch для {0} пользователей?"
  }
}
```

### Поля каталога

| Поле | Тип | Обязательное | Описание |
|------|-----|--------------|----------|
| `version` | string | Да | Семантическая версия (X.Y.Z) |
| `language` | string | Да | Код языка (например, "ru", "en") |
| `fallback` | string/null | Да | Код резервного языка или null |
| `messages` | object | Да | Объект с сообщениями |

## Скрипты управления

### Валидация переводов

Проверяет структуру JSON, наличие обязательных полей, консистентность плейсхолдеров:

```bash
cd i18n
npm install
npm run validate
```

### Анализ покрытия

Сравнивает все локали с reference (en-US) и показывает процент покрытия:

```bash
npm run coverage
```

Пример вывода:
```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Locale       │ Version    │ Language   │ Coverage   │ Missing    │ Extra      │
├──────────────────────────────────────────────────────────────────────────────┤
│ ru-RU        │ 1.0.0      │ ru         │ 100.00% ✅ │ 0          │ 0          │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Автоверсионирование

Бump версии всех каталогов одновременно:

```bash
# Patch bump (1.0.0 → 1.0.1)
npm run bump

# Minor bump (1.0.0 → 1.1.0)
npm run bump -- --minor

# Major bump (1.0.0 → 2.0.0)
npm run bump -- --major
```

## Интеграция с существующими скриптами

### Обновление ActivityWatch.Windows.Common.psm1

Замените хардкодные строки на вызовы i18n:

**До:**
```powershell
throw 'Run this script from an elevated PowerShell session.'
```

**После:**
```powershell
throw (Get-LocalizedString -Key "errors.admin_required")
```

### Обновление deploy-ensemble.ps1

**До:**
```powershell
Write-Host "Starting ActivityWatch deployment..." -ForegroundColor Cyan
```

**После:**
```powershell
Get-LocalizedInfo -Key "starting_deployment"
```

## Добавление нового языка

1. Скопируйте `en-US.json` как шаблон:
   ```bash
   cp i18n/en-US.json i18n/fr-FR.json
   ```

2. Отредактируйте `fr-FR.json`:
   - Измените `language` на `"fr"`
   - Установите `fallback` в `"en"`
   - Переведите все сообщения в `messages`

3. Проверьте валидность:
   ```bash
   npm run validate fr-FR.json
   ```

4. Протестируйте в PowerShell:
   ```powershell
   Initialize-Locale -Culture "fr-FR"
   ```

## Best Practices

### ✅ Делайте

- Используйте семантическое версионирование для каталогов
- Всегда указывайте fallback для resilience
- Группируйте сообщения по категориям (errors.*, info.*, etc.)
- Нумеруйте плейсхолдеры последовательно: `{0}`, `{1}`, `{2}`
- Проверяйте покрытие перед релизом (`npm run coverage`)

### ❌ Не делайте

- Не хардкодьте строки в коде скриптов
- Не смешивайте языки в одном сообщении
- Не пропускайте плейсхолдеры в переводах
- Не забывайте обновлять версию при изменении сообщений

## Troubleshooting

### Ошибка: "Localization file not found"

Убедитесь, что путь к i18n директории правильный:
```powershell
$env:I18N_ROOT = "C:\Path\To\i18n"
Initialize-Locale -Culture "ru-RU" -I18nRoot $env:I18N_ROOT
```

### Ошибка: "Failed to format message"

Проверьте соответствие количества аргументов плейсхолдерам:
```powershell
# ❌ Неправильно: 2 аргумента для 1 плейсхолдера
Get-LocalizedString -Key "errors.binary_missing" -FormatArgs @("file.exe", "extra")

# ✅ Правильно
Get-LocalizedString -Key "errors.binary_missing" -FormatArgs @("file.exe")
```

### Missing keys после обновления en-US

Запустите анализ покрытия и добавьте отсутствующие ключи:
```bash
npm run coverage
# Отредактируйте ru-RU.json, добавив missing keys
npm run validate
```

## Миграция с хардкодных строк

1. Экспортируйте существующие строки в шаблон:
   ```powershell
   Export-LocaleTemplate -OutputPath ".\i18n\template.json"
   ```

2. Заполните переводы

3. Постепенно заменяйте строки в коде на вызовы i18n функций

4. Протестируйте с обоими локалями

## Лицензия

MIT
