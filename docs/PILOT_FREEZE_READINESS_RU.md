# Pilot freeze readiness AWatch-rus

Статус: pilot freeze candidate.

Документ фиксирует gate перед заморозкой текущего `main` для пилота. Freeze не
добавляет новую функциональность, не меняет архитектуру и не упрощает
существующие контуры. Цель gate - подтвердить, что текущий main можно
демонстрировать и донастраивать в пилоте без расширения заявлений о продукте.

## 1. Границы продукта

AWatch-rus Pilot v1 позиционируется как Workforce-first платформа с отдельными
ролями Security и Forensics.

В рамках pilot freeze запрещено заявлять AWatch-rus как:

- SIEM;
- enterprise DLP;
- EDR/XDR;
- сертифицированное СЗИ;
- ML/LLM UEBA;
- готовый pfSense production ingestion или SIEM ingestion.

UEBA v1 является rule-based risk scoring для ручной проверки. Риск не является
автоматическим вердиктом и не доказывает нарушение без анализа человеком.

pfSense в текущем пилоте допускается только как
contract/readiness/optional integration layer. Его статус в demo/pilot
материалах: `contract_only` или `readiness`, если отдельный production
ingestion не включен и не принят отдельным gate.

## 2. Архитектурная фиксация

Pilot freeze candidate сохраняет текущую архитектуру:

- Rust Backend;
- Rust Agent;
- Rust-first operational CLIs and services;
- HTML/HTMX Portal;
- role-based Pilot v1;
- documented PowerShell runtime/fallback/installer/repair layer до отдельного
  retirement gate;
- future React/TypeScript UI только как future direction;
- Tauri только как future desktop Forensics;
- Dioxus не рассматривается.

Breaking changes, architecture changes и simplifications в рамках freeze gate
запрещены.

## 3. Роли Pilot v1

Ожидаемая видимость ролей:

| Роль | Видимость |
|---|---|
| `executive` | Только executive-level: главный вывод, агрегированные риски, итоговые управленческие действия без сырых технических очередей. |
| `manager` | Workforce/department/owner/activity: загрузка, подразделения, ответственные, тренды и управленческие отчеты. |
| `security` | Security/UEBA/pfSense readiness: candidates, severity, rule-based UEBA, security context и `contract_only` readiness. |
| `forensics` | Investigation/evidence/reporting: карточки расследований, timeline, evidence package и Markdown export. |
| `admin` | Административные и технические функции: полнота данных, качество сбора, источники, fallback, service/readiness status. |

Если документация или тесты расходятся с этой матрицей, исправлять нужно
документацию или тестовые ожидания. Бизнес-логику менять только при отдельном
подтвержденном дефекте.

## 4. Обязательные проверки

Перед фиксацией pilot freeze candidate выполнить:

```bash
cd adk-rust
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --release
```

Из корня репозитория выполнить:

```bash
bash scripts/verify_release_assets.sh dist/release-v0.2
bash scripts/check_private_config_guard.sh
bash scripts/check_production_inventory_placeholders.sh
bash scripts/check_demo_safety.sh
node scripts/pilot-validation-smoke.mjs
node scripts/detmir-pilot-demo-smoke.mjs
node scripts/detmir-portal-tabs-smoke.mjs
```

Для Windows/PowerShell слоя выполнить синтаксические и policy-safe проверки на
Windows-хосте или в CI-среде, где доступен PowerShell:

```powershell
Get-ChildItem .\windows -Filter *.ps1 -Recurse | ForEach-Object {
  $tokens = $null
  $errors = $null
  $null = [System.Management.Automation.Language.Parser]::ParseFile(
    $_.FullName,
    [ref]$tokens,
    [ref]$errors
  )
  if ($errors.Count -gt 0) { throw "$($_.FullName): $($errors[0].Message)" }
}
```

Также проверить release/demo artifacts:

```bash
git diff --check
bash scripts/check_demo_safety.sh
```

Ожидаемый результат для публичных tracked docs: нет live IP, hostnames, ФИО,
логинов сотрудников заказчика, названий подразделений заказчика, live case IDs,
runtime evidence paths, токенов и паролей. Допустимы только placeholders,
TEST-NET адреса, `.example` файлы и явно обезличенные demo fixtures.

## 5. PowerShell/Rust migration gate

Rust-primary направление сохраняется.

Это не заявление о полном удалении PowerShell. В текущем pilot freeze часть
PowerShell допустима как documented fallback/installer/support layer.

PowerShell scripts не удалять автоматически. Оставшиеся runtime/fallback пути
нельзя удалять до выполнения всех условий:

- Rust replacement имеет parity по данным и ошибкам;
- прошел burn-in на пилотном контуре;
- выполнен canary/rollback gate;
- rollback path документирован и проверен;
- Scheduled Tasks/Services больше не ссылаются на удаляемый script;
- acceptance gate явно разрешает удаление.

Installer/repair PowerShell layer можно заменять постепенно: Rust dry-run,
structured report, apply mode, затем удаление старого слоя только после
проверки ссылок и отката.

Полное удаление PowerShell не входит в текущий pilot freeze.

## 6. Demo data hygiene

Demo data, screenshots, evidence pack и отчеты не должны содержать:

- реальные IP-адреса;
- реальные hostnames;
- ФИО сотрудников заказчика;
- логины сотрудников заказчика;
- названия подразделений заказчика;
- реальные security events;
- live case IDs;
- runtime evidence paths.

Для сетевых примеров использовать только RFC 5737 TEST-NET адреса:
`192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`.

Автоматический gate:

```bash
bash scripts/check_demo_safety.sh
```

Gate проверяет корневой `README.md`, demo/pilot/customer-facing документы,
`docs/demo/`, `docs/fixtures/`, evidence-pack и screenshots metadata. Перед
публикацией demo-pack дополнительно нужен ручной визуальный просмотр PNG:
скрипт проверяет PNG-сигнатуру, размер и текстовые метаданные, но не выполняет
OCR содержимого изображения.

## 7. Known limitations

- Pilot v1 не является сертифицированным средством защиты информации.
- UEBA v1 rule-based и не использует ML/LLM claims.
- pfSense находится в `contract_only/readiness` контуре, если ingestion не
  включен отдельным production gate.
- Некоторые PowerShell runtime/fallback пути сохраняются до завершения
  burn-in/canary/rollback gate.
- Demo fixtures не являются production ingestion и не доказывают полноту
  production-интеграций.
- Security candidates и derived cases являются сигналами для ручной проверки,
  а не подтвержденными инцидентами.

## 8. Follow-up после freeze

- Update GitHub Actions versions / Node runtime deprecation cleanup.

Этот пункт является техническим долгом и не блокирует pilot freeze. Менять
workflow в рамках freeze можно только минимально, без изменения release
semantics и только при сохранении зеленого CI.

## 9. Freeze decision

Pilot freeze candidate можно фиксировать только если:

- обязательные проверки выполнены или documented exception внесен в release
  notes;
- границы продукта в README, pilot docs и acceptance docs согласованы;
- demo data hygiene подтверждена;
- PowerShell fallback не удален без gate;
- known limitations включены в материалы пилота.
