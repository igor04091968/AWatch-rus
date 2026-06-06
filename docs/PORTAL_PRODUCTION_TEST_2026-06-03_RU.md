# Боевая проверка AWatch-rus Portal, 2026-06-03

Документ фиксирует end-to-end проверку портала AWatch-rus в production-контуре без
публикации live IP, доменов, имен хостов и путей с персональными данными.

## Область проверки

- gateway entrypoint: `https://<PUBLIC_GATEWAY_FQDN>/portal/`;
- локальный backend портала на `<GATEWAY_HOST>`;
- evidence/readiness API на `<AW_SERVER_HOST>`;
- `detmir-check`, `detmir-auto`, `aw-rus-healthd`;
- вкладки портала: оператор, руководитель, владелец, инциденты ИБ, отчеты;
- API: health, summary, operator, manager, owner, incidents, reports,
  workforce policy explain, readiness bundle/verify, DLP evidence.

## Найденные проблемы

1. `aw-rus-healthd` запускался с public-safe placeholder значениями Windows
   target из публичных group vars. В результате healthd проверял не production
   host/buckets и давал ложный FAIL.
2. `detmir-check` считал AFK bucket строго fresh даже при отсутствии активной
   интерактивной сессии. Это создавало ложный FAIL для `detmir-auto` и портала.
3. `detmir-auto` и `detmir-portal` не имели единого приватного runtime env для
   `detmir-check`.
4. `detmir-portal` обрабатывал HTTP-запросы последовательно. Параллельные fetch
   из браузера могли блокироваться тяжелыми snapshot-route.

## Исправления

- Production runtime на AW-сервере восстановлен из приватного inventory/runtime
  values; публичные placeholders не записывались в Git.
- В `deploy_aw_server.yml` добавлена защита: playbook сохраняет уже рабочие
  runtime значения monitored Windows target и не принимает TEST-NET/example
  значения для production healthd.
- В `detmir-check` добавлен режим `interactive_fresh` для AFK/DLP heartbeat:
  при активной свежей worktime-сессии bucket обязан быть fresh; при отсутствии
  активной интерактивной сессии stale/missing bucket классифицируется как
  `INACTIVE` и не валит контур.
- В `detmir-check` добавлен `DETMIR_RDP_HOST` / `--rdp-host`, чтобы TCP checks
  не зависели от public placeholder defaults.
- На Proxmox создан приватный `/etc/detmir/detmir-check.env`; systemd units
  читают его через `EnvironmentFile`.
- `detmir-portal` переведен на concurrent request handling и короткий TTL-cache
  snapshot, чтобы несколько одновременных UI/API-запросов использовали один
  общий снимок состояния.

## Результаты проверки

- `aw-rus-healthd`: `ok=14`, `warn=0`, `fail=0`.
- `detmir-check`: `rc=0`, `bucket_stale=0`, `bucket_dead=0`,
  `service_failures=0`.
- `detmir-auto-rust --no-heal`: `severity=OK`, `needs_heal=false`,
  DLP `ok=22`, `warn=0`, `fail=0`.
- `detmir-auto.service`: `Result=success`, `ExecMainStatus=0`.
- Proxmox failed units: `0`.
- AW server failed units: `0`.
- Authenticated gateway smoke:
  - `/portal/` -> `200`;
  - `/portal/app.js` -> `200`;
  - `/portal/api/health` -> `200`;
  - `/portal/api/summary` -> `200`;
  - `/portal/api/operator` -> `200`;
  - `/portal/api/reports` -> `200`;
  - `/portal/api/readiness/bundle` -> `200`;
  - `/portal/api/readiness/verify` -> `200`;
  - `/portal/api/dlp/evidence` -> `200`.
- Parallel backend API smoke: all checked endpoints returned `200`; heavy
  snapshot endpoints completed from shared cache in about one snapshot window.
- Playwright browser smoke through `<PUBLIC_GATEWAY_FQDN>`:
  - title: `AWatch-rus Portal`;
  - JavaScript/page errors: none;
  - all checked `/portal/api/*` endpoints returned `200`;
  - tabs opened: руководитель, инциденты ИБ, отчеты;
  - report UI rendered `Markdown для отчета` and `Печать / PDF`;
  - screenshot saved as runtime artifact outside the repository.

## Ожидаемое поведение

- Unauthenticated `/portal/*` requests return `401`; this is normal gateway
  protection.
- Direct local portal API may expose only local evidence/readiness placeholders.
  Production UI must access readiness/evidence through gateway routes, where
  nginx proxies these paths to the AW evidence API.
- Public repository files must keep placeholders. Live endpoint values belong
  only to private inventory, systemd env files, and runtime state.
