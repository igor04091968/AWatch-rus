# Product Positioning: AWatch-rus

AWatch-rus позиционируется как рабочая платформа:

- Workforce Analytics;
- Security Analytics;
- Forensics.

Это не SIEM, не классический DLP, не EDR/XDR и не сертифицированная СЗИ.
Интеграции и providers со статусом `planned` или `future` не считаются
реализованной функциональностью.

## Статусы

- `implemented` - есть код, контракт, документация или проверенный runtime path.
- `planned` - направление предусмотрено архитектурой и требует реализации или
  расширения parity.
- `future` - возможное развитие продукта без текущего готового collector/API.

## Workforce Analytics

Целевая аудитория:

- руководители;
- руководители подразделений;
- эксплуатация, отвечающая за качество данных.

Задачи слоя:

- активность сотрудников;
- подразделения;
- KPI и управленческие показатели;
- тренды;
- нагрузка, перегрузка и недогрузка.

Implemented:

- Executive/Workforce portal views;
- Workforce reports и Markdown-отчет;
- role-based Pilot v1 contracts для executive/manager;
- Rust-first backend helpers для worktime/report layer.

Planned:

- agentless PowerShell/SSH/Syslog providers как источники пилотного обследования;
- расширение матрицы совместимости agent/server/portal;
- validation profiles для российских Linux-дистрибутивов.

Future:

- расширенные Enterprise connectors для HR/directory context;
- React/TypeScript Enterprise UI как отдельный будущий интерфейсный слой;
- дополнительные отраслевые KPI после отдельного согласования схем данных.

## Security Analytics

Целевая аудитория:

- ИБ;
- технический аудитор;
- эксплуатация, отвечающая за контроль отклонений.

Задачи слоя:

- риск-скоринг;
- UEBA;
- аномалии;
- контроль отклонений;
- кандидаты на проверку.

Implemented:

- UEBA Score v1 как объяснимая rule-based модель без ML/LLM;
- Security portal view;
- события безопасности и DLP/evidence как прикладные модули;
- аудит ручных решений по кандидатам.

Planned:

- PowerShell Provider для legacy/agentless Windows-источников;
- SSH Provider для read-only probes и existing logs;
- Syslog Provider для нормализации событий от внешних источников;
- pfSense readiness остается `contract_only`, без заявления реального ingestion.

Future:

- SIEM interoperability как внешняя интеграция, а не замена SIEM;
- VPN provider после отдельного контракта и тестов;
- SCUD provider после отдельного vendor/source contract.

## Forensics

Целевая аудитория:

- ИБ;
- внутренние расследования;
- технические ответственные за evidence package.

Задачи слоя:

- расследования;
- timeline;
- evidence package;
- корреляция событий;
- экспорт отчетов.

Implemented:

- Forensics portal view;
- карточки расследований и timeline;
- investigation pack;
- Markdown export;
- связка user / host / app / network event там, где такие данные есть в
  текущих источниках.

Planned:

- расширение bounded EVTX/Hayabusa upload parity;
- унификация evidence package acceptance для дополнительных источников;
- расширение Rust validation CLI для полного замещения legacy `.ps1` paths.

Future:

- Tauri Desktop Forensics как отдельный будущий desktop-контур;
- расширенная корреляция с VPN/SCUD/directory events после появления
  реализованных providers;
- offline forensic workbench без изменения Pilot v1 contracts.

## Коммерческая граница заявления

AWatch-rus уже является рабочей платформой Workforce + Security + Forensics.
Архитектура предусматривает расширение на агентные и agentless-источники
данных, но каждый новый источник должен отдельно получить:

- контракт данных;
- реализацию;
- тесты;
- документацию;
- smoke-проверку;
- честный статус готовности.

