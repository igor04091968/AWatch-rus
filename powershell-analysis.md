# Анализ DLP-скриптов PowerShell (работоспособность)

Дата анализа: **2026-05-04 (UTC)**

## Проверенный scope

- `windows/dlp-endpoint-signals-collector.ps1`
- `windows/file-operations-collector.ps1`
- `windows/dlp-policy.example.json`
- `windows/web-category-rules.example.json`

## Ключевой итог

DLP-скрипты в целом рабочие по архитектуре (heartbeat в ActivityWatch, policy-driven правила, cooldown, enforcement), но есть **критичный риск misconfiguration** и несколько эксплуатационных рисков.

---

## Что точно хорошо

1. В обоих коллекторах включены `Set-StrictMode -Version Latest` и `$ErrorActionPreference = 'Stop'`.
2. Есть отправка событий в отдельные bucket’ы (`aw-dlp-endpoint-signals_*`, `aw-dlp-incidents_*`, `aw-file-operations_*`).
3. В endpoint-коллекторе реализованы:
   - правила по буферу обмена / USB / печати,
   - suppression через cooldown (`Should-EmitByCooldown`),
   - опциональный screenshot capture при инциденте.
4. В file collector есть наблюдение за `Desktop/Documents/Downloads` через `FileSystemWatcher`.

---

## Найденные проблемы и риски

### 1) Критично: дефолтный путь конфига в endpoint-скрипте не совпадает с проектом

- `dlp-endpoint-signals-collector.ps1` использует по умолчанию:
  - `C:\ProgramData\ActivityWatch\deployment-config.json`
- Остальной проект использует namespace `AWatch-rus` (`C:\ProgramData\AWatch-rus\...`).

**Риск:** endpoint-коллектор может стартовать без нужного deployment-конфига и работать с неверными/пустыми параметрами.

### 2) Нет строгой проверки HTTP-результата в file collector

В `file-operations-collector.ps1` POST выполняется через `HttpClient`, но код ответа не валидируется (`IsSuccessStatusCode` не проверяется), ошибки частично только логируются.

**Риск:** «тихая» потеря telemetry при 4xx/5xx.

### 3) Watcher не снимает event subscriptions явно

Есть `Register-ObjectEvent`, но в `finally` disposal только watcher-объектов; отписка событий (`Unregister-Event`) явно не делается.

**Риск:** при рестартах/долгой работе возможно накопление подписок в сессии.

### 4) Screenshot/GUI-зависимость для enforcement

`Capture-IncidentScreenshot` и balloon notification завязаны на `System.Windows.Forms/System.Drawing`.

**Риск:** в non-interactive / service context часть enforcement UX может не работать (событие уйдёт, но скриншот/уведомление может не сформироваться).

---

## Рекомендации (приоритет)

1. **P1:** выровнять дефолтный `ConfigPath` в `dlp-endpoint-signals-collector.ps1` на `C:\ProgramData\AWatch-rus\deployment-config.json`.
2. **P1:** добавить проверку `response.IsSuccessStatusCode` в `file-operations-collector.ps1` и логировать body/status при ошибках.
3. **P2:** сохранить subscription-объекты `Register-ObjectEvent` и делать `Unregister-Event` в `finally`.
4. **P2:** для enforcement/UI добавить fallback режим «headless» (только лог + heartbeat).

---

## Что не удалось проверить в текущей среде

В этом контейнере отсутствует `pwsh`, поэтому не выполнены:

- синтаксический parse всех `*.ps1/*.psm1` через PowerShell parser;
- `Test-ModuleManifest`;
- smoke-run на Windows API (`Get-WinEvent`, `Get-Partition`, `Get-Disk`, `Set-Clipboard`, `Win32_PrintJob`).

---

## Команды для целевой Windows-проверки

```powershell
# 1) Синтаксис
Get-ChildItem .\windows -Recurse -Include *.ps1,*.psm1 | ForEach-Object {
  [void][System.Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$null,[ref]$errs)
  if($errs){ "FAIL $($_.FullName)" } else { "OK $($_.FullName)" }
}

# 2) Быстрый запуск file collector (с логом)
.\windows\file-operations-collector.ps1 -ConfigPath 'C:\ProgramData\AWatch-rus\deployment-config.json' -LogPath 'C:\ProgramData\AWatch-rus\collector-fileops.log'

# 3) Быстрый запуск endpoint collector (с логом)
.\windows\dlp-endpoint-signals-collector.ps1 -ConfigPath 'C:\ProgramData\AWatch-rus\deployment-config.json' -PolicyPath 'C:\ProgramData\AWatch-rus\dlp-policy.json' -LogPath 'C:\ProgramData\AWatch-rus\collector-endpoint.log'
```
