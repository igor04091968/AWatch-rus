# AWatch-rus Portal: рабочий кабинет

Дата: 2026-06-04

## Цель изменения

Портал переведен из технической "админки логов" в рабочий кабинет для трех ролей:

- руководитель: управленческая сводка, Workforce KPI, отклонения, отчеты;
- ИБ: риск-срез, DLP/evidence, состояние контрольных сигналов;
- расследователь: карточки инцидентов, evidence, хеши и выгрузка материалов.

## Что изменено

- Верхняя панель переименована в `AWatch-rus Workforce / Security / Forensics`.
- Добавлены элементы отчета: период, подразделение, ответственный, экспорт Markdown/PDF, статус сенсоров.
- Навигация приведена к продуктовой модели:
  - `Сводка`;
  - `Workforce`;
  - `Security`;
  - `Forensics`;
  - `Отчеты`.
- Первый экран показывает executive summary:
  - индекс активности за день;
  - активные сотрудники;
  - отклонения от нормы;
  - подразделения с просадкой;
  - риски ИБ;
  - новые инциденты;
  - готовность отчета;
  - доказательная база.
- Workforce-блок показывает загрузку, простои, перегруз, приложения, объяснение индекса и policy audit.
- Security-блок использует темную тему и выводит риск/ИБ-срез без автоматического воздействия на сеть.
- Forensics-блок показывает инциденты, evidence, скриншоты, хеши и ссылки на выгрузку.
- Отчеты дополнены типами: ежедневный, недельный, месячный, по подразделению, по сотруднику, по инциденту, акт пилота.

## Важные ограничения

- Фильтры периода, подразделения и ответственного пока являются UI-контролами. Backend-фильтрация должна добавляться отдельным изменением, чтобы не искажать текущие данные.
- `Индекс активности` остается proxy-метрикой: активное время / плановое время. Weighted KPI объясняется отдельно через role/application policy.
- Security/Forensics показывают риск-срез и evidence. Это не заявляется как сертифицированная СЗИ.
- pfSense и сетевой периметр этим изменением не затрагивались.

## Проверки

Локальные проверки:

```bash
export CARGO_TARGET_DIR=/tmp/detmir-adk-rust-target
node --check adk-rust/crates/detmir-portal/src/static/app.js
git diff --check
cargo test --manifest-path adk-rust/Cargo.toml -p detmir-portal
cargo clippy --manifest-path adk-rust/Cargo.toml -p detmir-portal --all-targets -- -D warnings
cargo build --release --manifest-path adk-rust/Cargo.toml -p detmir-portal
```

Боевые проверки после деплоя:

- `detmir-portal.service`: active;
- `nginx`: active;
- `detmir-portal-evidence.service`: active;
- `/portal/`: HTTP 200;
- `/portal/api/summary`: HTTP 200;
- `/portal/api/operator`: HTTP 200;
- `/portal/api/manager`: HTTP 200;
- `/portal/api/owner`: HTTP 200;
- `/portal/api/incidents`: HTTP 200;
- `/portal/api/reports`: HTTP 200;
- `/portal/api/readiness/bundle`: HTTP 200;
- `/portal/api/workforce/policy/explain`: HTTP 200;
- `/portal/api/dlp/evidence`: HTTP 200.

Playwright smoke:

- заголовок страницы: `AWatch-rus Portal`;
- вкладки `Сводка`, `Workforce`, `Security`, `Forensics`, `Отчеты` открываются;
- JS-ошибок нет;
- API-ответов с кодом 400+ нет;
- `Security` включает темную тему.
