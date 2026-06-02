# Cross-OS Native DLP Enforcement

## Цель

Расширять AWatch-rus как легкую DLP-систему без тяжелого монолитного endpoint-agent.
Базовый принцип: сначала использовать естественные механизмы ОС и управляемых браузеров,
а собственный код держать тонким слоем политики, телеметрии и корреляции.

Это не заменяет текущий Windows/RDP contour. Он остается основным production-контуром:

- `AWatchRusCollectorGuard` следит за collector health;
- `aw-rus-healthd` принимает решение по свежести бакетов и активности хоста;
- `tsj-guardian-bot` наблюдает и лечит только реальные деградации;
- DLP policy engine остается центральным источником правил.

## Единая модель политики

Новые OS-specific механизмы должны сводиться к одному контракту:

```json
{
  "nativeControls": {
    "mode": "monitor",
    "channels": {
      "removableStorage": {"action": "audit"},
      "print": {"action": "audit"},
      "clipboard": {"action": "audit"},
      "browserUpload": {"action": "audit"},
      "appExecution": {"action": "audit"}
    }
  }
}
```

Допустимые действия:

- `audit`: только событие в `aw-dlp-endpoint-signals_<host>`;
- `warn`: событие плюс локальное уведомление пользователя;
- `block`: блокировка штатным механизмом ОС или managed browser;
- `blockWithOverride`: блокировка с управляемым исключением, если платформа это поддерживает;
- `disabled`: канал выключен.

Для rollout по умолчанию используется `monitor/audit`. `block` включается только по одному
каналу и одной группе хостов после накопления baseline.

На Windows это правило enforced в `dlp-endpoint-signals-collector.ps1`: если endpoint-правило
просит `action: "block"`, но `nativeControls.mode` не равен `enforce` или канал не разрешает
`block`, collector подавляет enforcement и пишет инцидент с `enforcementSuppressed=true`.

## Windows

Windows остается самой зрелой платформой для ближайшего enforcement.

Предпочтительный порядок:

1. USB/removable storage:
   - мониторинг: текущий `dlp-endpoint-signals-collector.ps1`;
   - enforcement: GPO/registry/device installation restrictions и `Set-Disk -IsReadOnly`;
   - область блокировки: запись на removable media, не чтение.
2. Print:
   - мониторинг: `Microsoft-Windows-PrintService/Operational`;
   - enforcement: отмена print job штатным spooler API/CIM;
   - rollout: сначала только документы с совпадением `documentRegex`.
3. Clipboard:
   - мониторинг: текущий endpoint collector;
   - enforcement: очистка clipboard только для high-confidence правил;
   - риск: UX и ложные срабатывания, поэтому `block` не включать глобально.
4. App execution:
   - enforcement: AppLocker или WDAC/App Control for Business;
   - назначение: не “DLP content”, а сужение каналов утечки через запрещенные приложения.
5. Browser upload:
   - легкий путь: managed browser policy + URL/category rules;
   - сильный путь: расширение браузера или коммерческий DLP browser control, если нужен inline file upload block.

Ограничение: AppLocker/WDAC управляют запуском кода, но не поведением уже запущенного приложения.
Поэтому они дополняют DLP, а не заменяют USB/print/browser enforcement.

## macOS

Для macOS нельзя идти через kernel extension как основной путь. Современный естественный
вариант - System Extensions и профиль управления через MDM.

Предпочтительный порядок:

1. Monitor-only agent:
   - LaunchDaemon + Swift/Go helper;
   - публикация событий в AW buckets;
   - локальный health heartbeat по аналогии с `aw-rus-collector-guard_<host>`.
2. Endpoint Security system extension:
   - file open/write/exec telemetry;
   - deny только после отдельного PoC и подписи/entitlements;
   - обязательная MDM-подготовка approval profile.
3. Network Extension:
   - для DNS/proxy/web egress контроля;
   - лучше применять к managed domains, не как полный MITM по умолчанию.
4. MDM restrictions:
   - screen capture, AirDrop, external media, profile-level restrictions;
   - это самый легкий enforcement, если парк управляется MDM.

Ограничение: без MDM и Apple Developer entitlements macOS enforcement будет хрупким.
Для неуправляемых Mac оставляем monitor-only.

## Linux

Linux должен быть легким и дистрибутивно-нейтральным. Не начинать с “универсального агента,
который перехватывает все syscalls”.

Предпочтительный порядок:

1. fanotify file gate:
   - мониторинг и permission events для чувствительных каталогов;
   - хороший MVP для removable mount points, home/project shares, export directories;
   - блокировка через permission response до открытия файла.
2. auditd/journald collectors:
   - дешево для exec, sudo, mount, removable media, ssh/scp hints;
   - подходит для server/workstation baseline.
3. eBPF telemetry:
   - использовать для observability: process, connect, file metadata;
   - enforcement только через BPF LSM на поддерживаемых ядрах и после отдельной совместимости.
4. Desktop clipboard/print:
   - monitor-only через DE-specific tools (`wl-paste`, `xclip`, CUPS logs);
   - block режим только для управляемых рабочих станций.

Ограничение: Linux enforcement сильно зависит от ядра, LSM stack и дистрибутива. Поэтому
первая production-версия должна быть fanotify + auditd, а eBPF оставить как расширяемый слой.

## ChromeOS и managed browser

Для ChromeOS не надо писать свой endpoint-agent. Если устройства управляются через Google
Admin Console, использовать ChromeOS Data Controls:

- copy/paste;
- printing;
- screen capture/screen sharing;
- file open/upload/transfer;
- removable storage.

Для Windows/macOS/Linux браузерный контур должен быть policy-first:

- Edge/Chrome enterprise policies;
- URL/category lists из текущего DLP policy engine;
- расширение браузера только когда нужен inline upload/file decision, а не только telemetry.

## Priority Backlog

### P0 - сейчас

- Привести все health-check скрипты к единой inactive/guard-aware классификации.
- Хранить `nativeControls` в policy document как forward-compatible секцию.
- Не включать новый `block` глобально; только `audit` и `warn`.

### P1 - Windows production

- USB write-block profile через GPO/PowerShell.
- Print cancel по `documentRegex`.
- Browser upload detection по managed URL categories.
- Guard heartbeat для applied nativeControls version/checksum.

### P2 - Linux MVP

- `aw-linux-file-gate` на fanotify для monitor/audit по каталогам.
- `aw-linux-audit-collector` для mount/usb/scp/sudo/process telemetry.
- systemd unit + heartbeat bucket.

### P3 - macOS MVP

- `aw-macos-monitor` LaunchDaemon без enforcement.
- MDM profile checklist.
- System Extension PoC только для managed Macs.

### P4 - managed browser / ChromeOS

- Транслятор DLP policy domains/categories в Chrome/Edge policy bundle.
- ChromeOS Data Controls mapping для организаций с Google Workspace.

## Rollout Rules

- Каждая новая платформа стартует в `monitor`.
- `block` разрешен только после минимум 7 дней clean baseline.
- Любой block должен писать:
  - rule id;
  - action;
  - platform;
  - native mechanism;
  - enforcement result;
  - override id, если применимо.
- Если local guard/heartbeat старше порога, центральный бот не делает широкий restart,
  а переводит платформу в degraded и запускает platform-specific recovery.

## Sources

- Microsoft AppLocker overview: https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/applocker/applocker-overview
- Microsoft AppLocker policy design: https://learn.microsoft.com/en-za/windows/security/application-security/application-control/app-control-for-business/applocker/understand-applocker-policy-design-decisions
- Microsoft Purview Chrome DLP extension: https://learn.microsoft.com/en-us/purview/dlp-chrome-learn-about
- Apple system extensions deployment: https://support.apple.com/guide/deployment/system-extensions-in-macos-depa5fb8376f/web
- Linux fanotify manual: https://man7.org/linux/man-pages/man7/fanotify.7.html
- eBPF docs: https://docs.ebpf.io/
- ChromeOS Data Controls: https://support.google.com/chrome/a/answer/11587610
