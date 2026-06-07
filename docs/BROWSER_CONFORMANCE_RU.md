# Browser Conformance Smoke

`scripts/browser-conformance-smoke.mjs` проверяет, что ключевые блоки портала
AWatch-rus реально отображаются в браузере. Проверка не оценивает дизайн,
цвета, pixel-perfect верстку или бизнес-корректность данных.

## Что проверяется

- Executive view:
  - KPI;
  - Explainable KPI;
  - Risk Narrative;
  - Recommended Actions.
- Workforce view:
  - KPI;
  - сравнение подразделений;
  - статус тренда;
  - Explainable KPI.
- Security view:
  - события безопасности;
  - связь рисков и активности;
  - Recommended Actions для ИБ;
  - кандидаты на проверку.
- Forensics view:
  - расследования;
  - timeline событий;
  - материалы расследования;
  - аудит.

Также проверяется:

- страница не пустая;
- нет browser page errors;
- нет console errors;
- нет HTTP 500 responses;
- создаются screenshots.

## Как запускать

Сначала запустить портал AWatch-rus. Затем:

```bash
AWATCH_BROWSER_SMOKE_URL=http://127.0.0.1:8720/portal/ \
  node scripts/browser-conformance-smoke.mjs
```

Если портал защищен basic auth:

```bash
AWATCH_BROWSER_SMOKE_BASIC_AUTH='<BASE64_USER_PASSWORD>' \
AWATCH_BROWSER_SMOKE_URL=https://<PORTAL_HOST>/portal/ \
  node scripts/browser-conformance-smoke.mjs
```

## Артефакты

Скриншоты сохраняются в:

```text
artifacts/browser-smoke/
```

Минимальный набор:

- `executive.png`;
- `workforce.png`;
- `security.png`;
- `forensics.png`.

Каталог является runtime artifact и не предназначен для коммита в Git.

## Ограничения

- Smoke не добавляет новые API, UI, данные или claims.
- Smoke не доказывает production performance.
- Smoke не заменяет ручную демонстрационную проверку перед показом заказчику.
- Smoke не проверяет весь портал: только ключевые блоки Executive, Workforce,
  Security и Forensics.
- Risk Narrative проверяется в Executive view. Security view проверяет
  существующий блок связи рисков и активности, без ML/LLM claims.
