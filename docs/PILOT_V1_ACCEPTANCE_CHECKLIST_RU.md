# Pilot v1.0 Acceptance Checklist

## Назначение

Этот checklist закрепляет Pilot v1.0 как приемочный контур AWatch-rus для
демонстрации заказчику. Базовый кодовый контур: commit
`067ad0939c3b8c34df8af9ff55bb219277d08341`.

Pilot v1.0 принимается как Workforce Analytics + Security Analytics +
Forensics с ролевым порталом, стабильными JSON API, UEBA Score v1 и pfSense
readiness в режиме `contract_only`.

## Не входит в приемку

- SIEM.
- Классический DLP.
- ML/LLM scoring.
- Автоматическое сетевое воздействие.
- pfSense ingestion.
- NAC, SOAR, quarantine, изменение firewall/VPN/routing.
- Dioxus, React, Tauri, Electron как основной UI Pilot v1.0.

## Общие критерии

| Критерий | Ожидаемый результат | Статус |
| --- | --- | --- |
| Портал открывается | `/portal` загружается и показывает готовый статус данных | приемочный |
| Главный вывод | В Executive View отображается первым | приемочный |
| Русификация | В пользовательских блоках нет лишних англоязычных терминов | приемочный |
| Demo data | Нет реальных IP, hostname, логинов, ФИО, подразделений заказчика | приемочный |
| Документация | README и `docs/` содержат Pilot v1 материалы | приемочный |
| Неизменность позиционирования | Нет заявлений SIEM/DLP/pfSense ingestion | приемочный |

## Роли

| Роль | Endpoint/UI | Должна видеть | Не должна видеть по умолчанию |
| --- | --- | --- | --- |
| `executive` | Executive Dashboard | главный вывод, риски подразделений, Workforce summary | ИБ-детализацию и материалы расследований |
| `manager` | Workforce Portal | активность, подразделения, ответственных, тренды, Markdown-отчет | Security queue и evidence |
| `security` | Security Portal | кандидатов на проверку, risk score, события безопасности, аудит | управленческий Workforce Dashboard |
| `forensics` | Forensics Portal | карточки расследований, timeline, материалы, Markdown export | управленческие Workforce-разрезы |
| `admin` | Operations/Admin | качество данных, настройки, ClickHouse/fallback-статусы | не используется как управленческая роль |

Серверная проверка обязательна: роль должна ограничивать доступ на API-уровне,
а не только через скрытие кнопок в HTML.

## API contracts

Приемочные endpoint-ы:

- `/api/reports`;
- `/api/executive`;
- `/api/workforce`;
- `/api/security`;
- `/api/forensics`;
- `/api/ueba`;
- `/api/pfsense`;
- `/api/contracts/openapi.json`;
- `/api/contracts/typescript.d.ts`.

Ожидания:

- структуры JSON стабильны и additive-compatible;
- роль возвращается в `role_context`;
- неизвестные поля должны игнорироваться клиентом;
- отказ доступа возвращает 403;
- `/api/pfsense` возвращает `status=contract_only` и
  `ingestion_available=false`.

## UEBA Score v1

Приемочный UEBA-контур:

- rule-based;
- без ML;
- без LLM;
- score 0-100;
- severity: `normal`, `low`, `medium`, `high`, `critical`;
- reason codes;
- human-readable explanation;
- компоненты:
  - `activity_anomaly`;
  - `time_anomaly`;
  - `application_anomaly`;
  - `network_anomaly`;
  - `history_anomaly`.

UEBA Score v1 только ранжирует риск для ручной проверки. Он не блокирует
пользователей и не меняет сетевые политики.

## pfSense readiness

Приемочный статус: `contract_only`.

Допустимо:

- contracts;
- fixtures;
- docs;
- API-заготовка;
- demo-события только с RFC 5737 адресами.

Недопустимо:

- заявлять реальный ingestion без проверки;
- называть контур SIEM;
- обещать firewall/VPN enforcement;
- показывать реальные IP, hostname, login или события заказчика.

## Smoke acceptance

Команда:

```bash
node scripts/detmir-portal-tabs-smoke.mjs
```

Smoke должен подтвердить:

- портал открывается;
- Executive Dashboard открывается;
- Workforce/Manager view открывается;
- Security view открывается;
- Forensics view открывается;
- Admin/Operations view открывается;
- `/api/reports` возвращает валидный JSON;
- серверные role gates возвращают 403 для чужих срезов;
- demo-data не содержит чувствительных значений.

## Обязательные проверки перед приемкой

Выполнять из `adk-rust/`, кроме `git diff --check` и smoke:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
git diff --check
node scripts/detmir-portal-tabs-smoke.mjs
```

Приемка Pilot v1.0 считается готовой только при успешном прохождении всех
команд и отсутствии новых продуктовых обещаний за пределами текущего контура.
