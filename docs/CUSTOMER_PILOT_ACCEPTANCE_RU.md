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

- мониторинга активности рабочих мест;
- управленческой аналитики Workforce;
- технического аудита ИТ-контура;
- фиксации DLP-lite/ИБ-событий;
- просмотра evidence и отчетов в портале;
- контроля готовности системы через signed readiness bundle.

## 3. Состав поставки

| Компонент | Проверка |
|---|---|
| AWatch-rus portal | вход, роли оператора/руководителя/владельца |
| ActivityWatch telemetry | актуальность bucket/event данных |
| Workforce analytics | индекс активности, веса приложений, drill-down |
| DLP-lite incidents | USB/print/clipboard/file/email/browser signals, если включены |
| Evidence workflow | preview/download/view audit |
| Grafana dashboards | наличие данных и отсутствие query errors |
| Readiness bundle | checksum/signature/fingerprint |
| Prometheus alerts | readiness/signature alerts настроены |

## 4. Критерии приемки

Пилот считается успешным, если:

- портал доступен ответственным пользователям заказчика;
- telemetry freshness находится в согласованных пределах;
- readiness status = `OK` или все `WARN` имеют согласованный план устранения;
- signed readiness bundle проходит проверку;
- не менее одного управленческого отчета сформировано и принято заказчиком;
- не менее одного test incident/evidence workflow пройден end-to-end;
- заказчик подтвердил, что состав данных и уведомлений соответствует правилам
  внутреннего контроля и локальным нормативным документам.

## 5. Результаты проверок

| Проверка | Результат | Комментарий |
|---|---|---|
| Portal login | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Readiness bundle | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Workforce report | `<OK/WARN/FAIL>` | `<COMMENT>` |
| DLP-lite incident | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Evidence preview/download | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Grafana dashboards | `<OK/WARN/FAIL>` | `<COMMENT>` |
| Alerting | `<OK/WARN/FAIL>` | `<COMMENT>` |

## 6. Ограничения пилота

- AWatch-rus не заявляется как сертифицированная СЗИ, SIEM, EDR/XDR или
  enterprise DLP.
- pfSense/network quarantine интеграции являются опциональным интеграционным
  слоем и не входят в обязательный состав пилота.
- Telegram runtime может использоваться как интеграционный канал уведомлений,
  но не является ядром продукта.
- Результаты Workforce analytics являются управленческими proxy-метриками и
  должны трактоваться с учетом ролей, весов приложений и локальных регламентов.

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
