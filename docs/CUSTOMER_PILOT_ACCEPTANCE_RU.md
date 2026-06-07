# Акт приемки пилота AWatch-rus

Документ-шаблон для коммерческого пилота AWatch-rus. Перед передачей
заказчику заполнить реквизиты сторон, даты, состав стенда и результаты
проверок. Публичная версия не должна содержать live IP, домены, ФИО
сотрудников, case IDs, evidence paths или screenshots с реальными данными.

## 1. Стороны

| Поле | Значение |
|---|---|
| Заказчик | `<CUSTOMER_LEGAL_NAME>` |
| Исполнитель/правообладатель | `<RIGHT_HOLDER_LEGAL_NAME>` |
| Проект | AWatch-rus, программный продукт |
| Период пилота | `<PILOT_START_DATE>` - `<PILOT_END_DATE>` |
| Контур | `<PILOT_CONTOUR_NAME>` |

## 2. Цель пилота

Проверить применимость AWatch-rus для:

- Workforce Analytics: активность, подразделения, нагрузка и управленческий
  Markdown-отчет;
- Security Analytics: кандидаты на проверку, объяснимый risk score, события
  безопасности и аудит решений;
- Forensics: карточка расследования, timeline, evidence package и экспорт
  Markdown-отчета;
- Operations/Admin: качество данных, полнота данных, ClickHouse/fallback-статус
  и ошибки сбора;
- проверки ролевых ограничений на сервере, а не только в интерфейсе.

## 3. Состав поставки

| Компонент | Проверка |
|---|---|
| AWatch-rus portal | вход, `/portal`, переключение ролей `executive` / `manager` / `security` / `forensics` / `admin` |
| Executive Dashboard | главный вывод отображается первым, затем риски подразделений и краткий статус |
| Workforce analytics | индекс активности, сравнение подразделений, тренды, перегруз/недогруз, Markdown-отчет |
| Security analytics | кандидаты на проверку, UEBA Score v1, severity, аудит решений |
| Forensics workflow | карточка расследования, timeline, evidence package, Markdown export |
| Operations/Admin | полнота данных, качество данных, ClickHouse/fallback-статус, ошибки сбора |
| pfSense readiness | только `contract_only`: contracts/fixtures/API-заготовка, без production ingestion |

## 4. Критерии приемки

Пилот считается успешным, если:

- портал доступен ответственным пользователям заказчика;
- главный вывод в Executive View отображается первым;
- роли ограничивают доступ на API-уровне;
- качество и полнота данных имеют понятный статус;
- не менее одного управленческого отчета сформировано и принято заказчиком;
- не менее одного security candidate / investigation / evidence workflow
  пройден end-to-end;
- UEBA Score v1 остается rule-based и не заявляет ML/LLM;
- pfSense readiness остается `contract_only` и не заявляет production ingestion;
- заказчик подтвердил, что состав данных и уведомлений соответствует правилам
  внутреннего контроля и локальным нормативным документам.

## 5. Результаты проверок

| Проверка | Результат | Комментарий |
|---|---|---|
| Portal access | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Executive View | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Workforce / Manager View | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Security View | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Forensics View | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Operations/Admin View | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Role gates / API 403 | `<OK/WARN/FAIL>` | `<COMMENT>` |
| `/api/reports` JSON | `<OK/WARN/FAIL>` | `<COMMENT>` |
| `/api/pfsense` contract_only | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Demo data sanitization | `<OK/WARN/FAIL>` | `<COMMENT>` |

## 6. Ограничения пилота

- AWatch-rus не заявляется как сертифицированная СЗИ, SIEM, EDR/XDR или
  enterprise DLP.
- pfSense readiness в Pilot v1 является `contract_only`: contracts, fixtures,
  docs и API-заготовка без production ingestion, NAC, SOAR, quarantine и
  изменения firewall/VPN/routing.
- Grafana, Prometheus, Telegram и внешние интеграции могут использоваться в
  отдельных эксплуатационных контурах, но не являются обязательной частью
  приемки Pilot v1 demo pack.
- Результаты Workforce analytics являются управленческими proxy-метриками и
  должны трактоваться с учетом ролей, весов приложений и локальных регламентов.
- UEBA Score v1 ранжирует риск для ручной проверки и не принимает
  автоматических решений.

## 7. Решение

| Решение | Отметка |
|---|---|
| Пилот принят без замечаний | `<YES/NO>` |
| Пилот принят с замечаниями | `<YES/NO>` |
| Требуется доработка | `<YES/NO>` |
| Рекомендуется коммерческое внедрение | `<YES/NO>` |

## 8. Подписи

| Сторона | ФИО/должность | Подпись | Дата |
|---|---|---|---|
| Заказчик | `<CUSTOMER_SIGNER>` |  |  |
| Исполнитель | `<CONTRACTOR_SIGNER>` |  |  |
