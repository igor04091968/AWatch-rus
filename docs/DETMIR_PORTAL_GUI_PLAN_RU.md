# DetMir Portal GUI Plan

Документ предназначен для следующего агента, в том числе менее сильного. Не
надо угадывать архитектуру: идти по фазам, проверять каждый слой, не ломать
текущий контур.

## Статус На 2026-06-02

Read-only MVP выполнен и развернут:

- Rust crate: `adk-rust/crates/detmir-portal`;
- production service: `detmir-portal.service` на Proxmox;
- bind: `127.0.0.1:8720`;
- gateway route: `https://<PUBLIC_GATEWAY_FQDN>/portal/`;
- API: `/api/health`, `/api/summary`, `/api/operator`, `/api/manager`,
  `/api/owner`, `/api/incidents`, `/api/links`;
- UI tabs: `Оператор`, `Руководитель`, `Владелец`, `Инциденты ИБ`;
- verification: local Rust gates OK, Ansible deploy OK, gateway health OK,
  Playwright desktop/mobile smoke OK.

Следующий агент не должен начинать MVP заново. Работать дальше от deployed
baseline и раздела `Phase 8: Post-MVP Enhancements`.

## Цель

Сделать единый web GUI для работы с контуром DetMir:

- оператор видит техническое состояние и последние проблемы;
- менеджер видит работу сотрудников и отклонения без технических терминов;
- владелец видит короткую управленческую картину: работа, риски, 1С, ИБ,
  проблемные зоны;
- текущие Grafana/AW/DLP/1C экраны не удаляются, а становятся источниками и
  deep links;
- первый production MVP только read-only.

## Жесткие ограничения

- pfSense не трогать.
- Telegram runtime остается Python.
- Не менять маршрутизацию, DNS, VPN, NAT и default routes.
- Не удалять Grafana dashboards, AW DB, SQLite DB, backups.
- Не добавлять write/action endpoints в первом MVP.
- Не выводить секреты в HTML, JSON, journald, git diff.
- Любая кнопка с мутацией только после отдельной фазы и отдельной проверки.
- Портал должен работать автономно на сервере, без зависимости от ноутбука.

## Текущий фундамент

Уже есть зеленые источники, которые портал должен использовать:

- `detmir-status --json` на Proxmox;
- `detmir-check --json` на Proxmox, включая `grafana-data`;
- `detmir-grafana-check` в Grafana CT 201;
- AW Worktime API на `<AW_SERVER_HOST>:5610`;
- ActivityWatch API на `<AW_SERVER_HOST>:5600`;
- DLP health/case/policy services на AW server;
- 1C analytics API на `<GATEWAY_HOST>:8710`;
- внешний gateway `https://<PUBLIC_GATEWAY_FQDN>/`;
- nginx Basic Auth на gateway.

## Целевая архитектура MVP

Новый Rust crate:

```text
adk-rust/crates/detmir-portal/
```

Production binary:

```text
/usr/local/bin/detmir-portal
```

Production service на Proxmox host:

```text
detmir-portal.service
```

Bind:

```text
127.0.0.1:8720
```

External route через существующий nginx gateway:

```text
https://<PUBLIC_GATEWAY_FQDN>/portal/
```

Почему Proxmox host:

- там уже живут `detmir-status`, `detmir-check`, `detmir-auto`;
- там есть доступ к `pct exec 201` для Grafana-check artifact;
- там уже стоит nginx gateway;
- не нужна новая CT/platform операция.

## Rust Stack

Предпочтение: использовать текущий стиль проекта.

Минимальный MVP:

- `tiny_http` для HTTP server, как в `worktime-api`, `dlp-policy-engine`,
  `dlp-case-management`;
- `reqwest blocking` для HTTP к AW/Grafana/1C/DLP;
- `serde`/`serde_json` для typed models;
- static HTML/CSS/JS встроить в binary через `include_str!`;
- без npm/build pipeline на первом этапе.

Не использовать на MVP:

- React/Vite/Next;
- отдельный frontend build;
- database для портала;
- websocket;
- сложный RBAC;
- write actions.

## Основные URL Портала

HTML:

```text
GET /portal/
GET /portal/operator
GET /portal/manager
GET /portal/owner
GET /portal/incidents
```

JSON API:

```text
GET /portal/api/health
GET /portal/api/summary
GET /portal/api/operator
GET /portal/api/manager
GET /portal/api/owner
GET /portal/api/incidents
GET /portal/api/links
```

Service-local direct URLs:

```text
GET /
GET /operator
GET /manager
GET /owner
GET /incidents
GET /api/health
GET /api/summary
```

Портал должен корректно работать за prefix `/portal/`. Не хардкодить абсолютные
пути вида `/api/...` в JS; использовать относительные `api/...` или вычислять
base path.

## Data Contract

### `/api/health`

Минимальный JSON:

```json
{
  "ok": true,
  "generated_at_utc": "2026-06-02T18:00:00Z",
  "version": "0.1.0",
  "sources": {
    "detmir_status": true,
    "detmir_check": true,
    "grafana_check": true,
    "worktime_api": true,
    "dlp_health": true,
    "one_c": true
  }
}
```

### `/api/summary`

Единый верхнеуровневый статус:

```json
{
  "severity": "OK",
  "operator_ok": true,
  "headline": "Контур работает штатно",
  "generated_at_utc": "2026-06-02T18:00:00Z",
  "blocks": {
    "collection": {"status": "OK", "text": "Данные свежие"},
    "grafana": {"status": "OK", "text": "7 панелей, данные актуальны"},
    "dlp": {"status": "OK", "text": "22 проверки OK"},
    "worktime": {"status": "OK", "text": "Есть данные за сегодня"},
    "one_c": {"status": "OK", "text": "API 1C analytics отвечает"}
  }
}
```

### `/api/operator`

Для оператора:

- `detmir_status`;
- `detmir_check.summary`;
- `grafana_data`;
- failed units count;
- freshness по buckets;
- последние WARN/FAIL;
- ссылки на Grafana/AW/worktime.

### `/api/manager`

Для менеджера:

- сотрудники за сегодня;
- активное время;
- доказанная работа;
- приложения;
- последние действия;
- простые отклонения:
  - данных нет;
  - данные устарели;
  - активность ниже ожидаемой;
  - слишком много событий DLP.

Источник: сначала AW Worktime API `/reports/worktime/today`. Не ходить напрямую
в SQLite.

### `/api/owner`

Для владельца:

- 4-6 крупных карточек:
  - "Работа сегодня";
  - "Риски ИБ";
  - "1С / финансы";
  - "Сбор данных";
  - "Инциденты ИБ";
  - "Что требует внимания";
- короткие выводы в человеческом языке;
- links на глубокие Grafana/1C/AW страницы.

### `/api/incidents`

MVP read-only список:

- DLP incidents/cases;
- DetMir health failures;
- Grafana data failures;
- stale collectors;
- 1C analytics warnings.

Пока без кнопок "закрыть", "назначить", "эскалировать".

## Источники Данных

### Proxmox local commands

```bash
detmir-status --json
detmir-check --json
systemctl --failed --no-pager
```

Правило:

- command timeout максимум 10 секунд;
- ошибка источника не должна валить HTTP server;
- ошибка источника должна попасть в JSON как `status: "FAIL"` или
  `source_error`.

### Grafana check artifact

Через `detmir-check` уже есть агрегированный `grafana-data`.

Для подробного operator view можно дополнительно читать:

```bash
sudo -n /usr/sbin/pct exec 201 -- cat /var/lib/detmir-grafana-check/latest.json
```

Если это не работает, не делать repair. Просто показать ошибку.

### AW Worktime API

Основной URL:

```text
http://<AW_SERVER_HOST>:5610/reports/worktime/today
```

Правило:

- `Connection: close`;
- timeout 10-15 секунд;
- не кешировать пустой отчет как успешный;
- если API временно не ответил, показать stale/source error.

### DLP

Минимум:

```bash
ssh aw-server 'sudo -n /usr/local/bin/dlp-health-check --json'
```

Лучше после MVP сделать HTTP client к case/policy APIs, но не обязательно в
первом проходе.

### 1C Analytics

Минимум:

```text
http://<GATEWAY_HOST>:8710/api/health
http://<GATEWAY_HOST>:8710/manager/brief
http://<GATEWAY_HOST>:8710/manager/actions
```

Если `/manager/brief` HTML, для MVP не парсить его глубоко. Дать link и health
card.

## UI Правила

Это рабочий портал, не landing page.

- Первый экран сразу показывает состояние контура.
- Без маркетингового hero.
- Без декоративных gradient/orb/background.
- Не делать карточки внутри карточек.
- Dense, спокойный, операционный интерфейс.
- Использовать 8px radius или меньше для cards/panels.
- Текст не должен вылезать из блоков.
- Цвета статусов:
  - OK: зеленый;
  - WARN: желтый/янтарный;
  - FAIL: красный;
  - UNKNOWN: серый.
- Не использовать одну сплошную синюю/фиолетовую палитру.
- Главные роли должны быть tabs/segmented control:
  - Оператор;
  - Руководитель;
  - Владелец;
  - Инциденты ИБ.
- Иконки можно использовать inline SVG только если нет frontend dependency.
  Если позже появится frontend dependency, использовать lucide icons.

## Экран 1: Operator Console

Цель: за 10 секунд понять, жив ли контур.

Блоки:

1. Верхняя строка:
   - общий статус;
   - время последней проверки;
   - `ok_for_operator`;
   - кнопка-ссылка "Открыть Grafana";
   - кнопка-ссылка "Открыть AW".

2. Health grid:
   - DetMir;
   - ActivityWatch;
   - Worktime API;
   - Grafana Data;
   - DLP;
   - 1C.

3. Data freshness:
   - buckets OK/STALE/DEAD;
   - Grafana freshness;
   - DLP counters.

4. Problems:
   - список WARN/FAIL;
   - для каждого: источник, текст, время, ссылка.

5. Deep links:
   - DetMir ActivityWatch dashboard;
   - Worktime report;
   - AW UI;
   - Grafana dashboards;
   - 1C brief.

MVP без кнопок restart/heal.

## Экран 2: Manager View

Цель: понять работу сотрудников без технических терминов.

Блоки:

1. Сегодня:
   - всего активного времени;
   - число пользователей;
   - последний сбор данных.

2. Сотрудники:
   - имя;
   - активное время;
   - последнее действие;
   - основные приложения;
   - статус данных.

3. Приложения:
   - 1С;
   - браузер;
   - Проводник;
   - прочие.

4. Отклонения:
   - нет данных;
   - неактивен;
   - слишком старый сбор;
   - DLP signal.

Не показывать:

- bucket ids;
- raw JSON;
- systemd unit names;
- stack traces.

## Экран 3: Owner View

Цель: дать владельцу управленческий ответ, а не технический dashboard.

Блоки:

1. "Компания сегодня":
   - работа идет / есть сбои / есть риски.

2. "Люди и работа":
   - активность;
   - заметные отклонения;
   - кто требует внимания.

3. "Безопасность":
   - DLP OK/WARN/FAIL;
   - открытые кейсы;
   - критичные сработки.

4. "1С и финансы":
   - health 1C analytics;
   - ссылка на financial/reporting board;
   - ссылка на actions.

5. "Что сделать":
   - 3-5 коротких рекомендаций.

Рекомендации в MVP должны быть rule-based, не AI:

- если `ok_for_operator=false`: "Проверить технический контур";
- если Grafana stale: "Обновить/проверить Grafana data pipeline";
- если DLP fail: "Открыть DLP обзор";
- если worktime rows empty: "Проверить RDP collectors".

## Экран 4: Incidents

MVP read-only.

Таблица:

- статус;
- тип;
- источник;
- краткое описание;
- время;
- ссылка.

Типы:

- `health`;
- `grafana`;
- `dlp`;
- `worktime`;
- `one_c`;
- `collector`.

Нельзя делать в MVP:

- закрытие инцидента;
- изменение severity;
- отправка в Telegram;
- запуск heal.

## Phase 0: Baseline Before Coding

Команды:

```bash
cd <PROJECT_ROOT>
git status --short
export CARGO_TARGET_DIR=<OPERATOR_HOME>/.cache/detmir-adk-rust-target
cd ansible
export no_proxy='localhost,127.0.0.1,<WINDOWS_HOST>,<AW_SERVER_HOST>,<GATEWAY_HOST>,<SERVER_SUBNET_CIDR>,<ENDPOINT_SUBNET_CIDR>'
export NO_PROXY="$no_proxy"
ansible proxmox -i inventory.ini -m shell -a 'detmir-status --json'
ansible proxmox -i inventory.ini -m shell -a 'detmir-check --json'
```

Acceptance:

- рабочее дерево понятно;
- текущий DetMir не сломан;
- нет попытки чинить unrelated проблемы.

## Phase 1: Create Rust Crate

Files:

```text
adk-rust/Cargo.toml
adk-rust/crates/detmir-portal/Cargo.toml
adk-rust/crates/detmir-portal/src/main.rs
adk-rust/crates/detmir-portal/src/static/index.html
adk-rust/crates/detmir-portal/src/static/app.css
adk-rust/crates/detmir-portal/src/static/app.js
```

Minimum CLI:

```text
detmir-portal --bind 127.0.0.1:8720
detmir-portal --bind 127.0.0.1:8720 --json-smoke
detmir-portal --help
```

Cargo deps:

```toml
anyhow.workspace = true
chrono.workspace = true
clap.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
tiny_http.workspace = true
```

Acceptance:

```bash
cd adk-rust
cargo fmt --all -- --check
CARGO_TARGET_DIR=<OPERATOR_HOME>/.cache/detmir-adk-rust-target cargo test -p detmir-portal
CARGO_TARGET_DIR=<OPERATOR_HOME>/.cache/detmir-adk-rust-target cargo clippy -p detmir-portal --all-targets -- -D warnings
CARGO_TARGET_DIR=<OPERATOR_HOME>/.cache/detmir-adk-rust-target cargo build --release -p detmir-portal
```

## Phase 2: Backend Aggregation

Implement typed structs:

```text
PortalSummary
PortalBlock
OperatorView
ManagerView
OwnerView
IncidentItem
SourceStatus
```

Implement source functions:

```text
read_detmir_status()
read_detmir_check()
read_grafana_check()
fetch_worktime_today()
fetch_one_c_health()
fetch_dlp_health()
build_incidents()
```

Rules:

- every source has timeout;
- every source returns typed `SourceStatus`;
- never panic on bad source JSON;
- never expose credentials;
- if one source fails, portal still responds with degraded status.

Acceptance:

```bash
detmir-portal --json-smoke
curl -fsS http://127.0.0.1:8720/api/health | jq .
curl -fsS http://127.0.0.1:8720/api/summary | jq .
curl -fsS http://127.0.0.1:8720/api/operator | jq .
```

## Phase 3: Static UI MVP

Implement one HTML app with tabs.

Required visible text:

- "Оператор";
- "Руководитель";
- "Владелец";
- "Инциденты ИБ";
- "Контур";
- "Данные";
- "Риски";
- "Работа сегодня".

Required behavior:

- load `/api/summary`;
- load active tab API;
- show loading state;
- show source error state;
- refresh every 60 seconds;
- links open existing systems.

No build step.

Acceptance:

```bash
curl -fsS http://127.0.0.1:8720/ | grep -F 'Оператор'
curl -fsS http://127.0.0.1:8720/ | grep -F 'Руководитель'
curl -fsS http://127.0.0.1:8720/ | grep -F 'Владелец'
```

## Phase 4: Browser Verification

Use Playwright only after service works by curl.

Desktop viewport:

```text
1440x1000
```

Mobile viewport:

```text
390x844
```

Check:

- page is nonblank;
- no text overlap;
- tabs work;
- all API requests return 200;
- no console errors;
- status and cards visible;
- external links are present.

Save screenshots under:

```text
.playwright-cli/
```

Do not commit screenshots unless explicitly requested.

## Phase 5: Deployment

Add playbook:

```text
ansible/deploy_detmir_portal.yml
```

Deploy to Proxmox host:

```text
/usr/local/bin/detmir-portal
/etc/systemd/system/detmir-portal.service
```

Service:

```ini
[Unit]
Description=DetMir Operator Portal
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=-/etc/detmir-portal.env
ExecStart=/usr/local/bin/detmir-portal --bind 127.0.0.1:8720
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

Nginx route:

```text
location /portal/ {
    proxy_pass http://127.0.0.1:8720/;
}
```

Important:

- preserve existing Basic Auth;
- do not bypass gateway auth;
- do not expose raw internal ports publicly.

Acceptance:

```bash
systemctl is-active detmir-portal
curl -fsS http://127.0.0.1:8720/api/health
curl -k -I -H 'Host: <PUBLIC_GATEWAY_FQDN>' https://127.0.0.1/portal/
```

External:

```text
https://<PUBLIC_GATEWAY_FQDN>/portal/
```

## Phase 6: Integrate Into Health Gates

Add `detmir-portal-check` only after MVP is stable, or add a mode inside
`detmir-portal`:

```bash
detmir-portal --check
```

It should verify:

- service can aggregate all required sources;
- `/api/health` is OK;
- HTML includes role tabs;
- gateway route returns 401 without auth or 200 with auth.

Then add artifact requirement:

```text
scripts/check_detmir_rust_release_artifacts.sh -> detmir-portal
```

Add required service check to `detmir-check` only after production service is
stable for at least one run.

## Phase 7: Rollback

Rollback must be simple:

```bash
sudo systemctl disable --now detmir-portal.service
sudo rm -f /usr/local/bin/detmir-portal
sudo rm -f /etc/systemd/system/detmir-portal.service
sudo systemctl daemon-reload
```

If nginx was changed:

- keep backup before edit;
- restore previous gateway config;
- `nginx -t`;
- `systemctl reload nginx`.

Never rollback by deleting unrelated gateway routes.

## Phase 8: Post-MVP Enhancements

Only after read-only portal is stable:

1. role-aware views based on gateway username;
2. incident comments;
3. acknowledge/assign incident;
4. safe "run check now";
5. safe "open Telegram status";
6. PDF/HTML daily owner report;
7. historical trends;
8. AI summary with strict source citations;
9. action buttons with explicit allowlist and audit log.

## Definition Of Done For MVP

MVP is done only when all are true:

- `detmir-portal` crate exists and builds in release;
- unit tests pass;
- clippy passes with `-D warnings`;
- portal serves HTML and JSON locally;
- portal deployed as systemd service on Proxmox;
- gateway URL works:
  `https://<PUBLIC_GATEWAY_FQDN>/portal/`;
- browser screenshots checked desktop and mobile;
- no secrets in HTML/JSON/journald;
- `detmir-status` stays OK after deployment;
- runbook updated;
- git commit pushed.

## Recommended First Implementation Slice

Do not start with all screens. First slice:

1. create `detmir-portal` crate;
2. implement `/api/health`;
3. implement `/api/summary`;
4. implement one static `/` page with four tabs but only Operator tab filled;
5. run local curl tests;
6. deploy service internally on Proxmox;
7. add `/portal/` gateway route;
8. browser-test external URL;
9. only then fill Manager and Owner views.

This keeps risk low and gives a visible result quickly.
