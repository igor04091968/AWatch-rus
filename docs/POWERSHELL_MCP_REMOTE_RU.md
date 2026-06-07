# AWatch-rus PowerShell MCP Remote

## Что это

Канонический путь для интерактивной PowerShell-работы с `AWatch-rus` Windows-хостом `<WINDOWS_HOST>` из Linux/Codex.

Это не замена `WinRM` в Ansible. Разделение теперь такое:

- `Ansible deploy / validation` — через `WinRM` (`5985`);
- `PowerShell MCP / operator shell / Codex` — через `SSH` (`22`) на том же хосте.

## Зафиксированная целевая Windows

Проверено на `2026-05-26`:

- host: `<WINDOWS_HOST>`
- product: `Windows Server 2025 Datacenter Evaluation`
- release: `24H2`
- build: `10.0.26100.32690`
- remote PowerShell: `5.1.26100.32684`
- `sshd`: `Running`

## Почему не WSMan

На текущем Linux admin-host локальный `pwsh 7.6.1` не поднимает `New-PSSession` к этому Windows-хосту без отдельного WSMan client stack (`libpsrpclient` / `PSWSMan`).

Практический вывод для AWatch-rus:

- не строить interactive MCP-путь вокруг `WSMan`;
- считать `SSH + powershell.exe` каноническим transport для локального `powershell-windows`;
- `WinRM` оставить для Ansible playbook-ов и коротких Windows-probes.

## Файлы проекта

- profile snippet: `scripts/powershell/detmir-powershell-profile.ps1`
- local config template: `scripts/powershell/detmir-windows.psd1.example`
- installer: `scripts/install_detmir_powershell_mcp.sh`

Локальные файлы после установки:

- `~/.config/powershell/Microsoft.PowerShell_profile.ps1`
- `~/.config/powershell/detmir-windows.psd1`

`detmir-windows.psd1` должен оставаться локальным секретным файлом с правами `600`.

## Установка

Из корня проекта:

```bash
bash scripts/install_detmir_powershell_mcp.sh
```

Installer:

- прописывает loader в локальный PowerShell profile;
- создаёт `~/.config/powershell/detmir-windows.psd1`, если его ещё нет;
- пытается заполнить `Host/User/Password` из `ansible/inventory.ini` группы `[aw_windows]`;
- если inventory не подходит, оставляет template со значением `CHANGE_ME`.

После установки:

- переоткрыть `pwsh`, либо
- перезапустить Codex session, чтобы `powershell-windows` перечитал профиль.

## Рабочие команды

В локальном `pwsh` и в `powershell-windows` MCP:

```powershell
detmir-win-test
detmir-win-ps '$PSVersionTable.PSVersion.ToString(); hostname; Get-Date -Format o'
detmir-win-ps 'Get-Service sshd | Select-Object Status,Name'
detmir-win-shell
```

Назначение:

- `detmir-win-test` — быстрый smoke-test удалённого PowerShell;
- `detmir-win-ps` — выполнить PowerShell-скрипт на `<WINDOWS_HOST>`;
- `detmir-win-shell` — открыть raw SSH shell на Windows-хост;
- `detmir-win-ssh` — выполнить произвольную SSH-команду одной строкой;
- `detmir-win-target` — показать текущий target.

## Быстрая проверка

Ожидаемый ответ:

```powershell
detmir-win-test
```

```text
5.1.26100.32684
Microsoft Windows NT 10.0.26100.0
Windows Server 2025 Datacenter Evaluation
```

## Операторское правило

Если нужен:

- массовый deploy toolkit;
- validation через playbook;
- работа с `ansible aw_windows`;

использовать `WinRM`.

Если нужен:

- интерактивный PowerShell из Linux;
- запуск удалённых PowerShell-скриптов из Codex/MCP;
- быстрая operator automation без `pywinrm`;

использовать `SSH`-путь из этого документа.
