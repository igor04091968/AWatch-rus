# Workforce catalogs

Эти TSV-файлы являются source of truth для справочников `aw_workforce`.

## Операции администратора

- Добавить категорию: добавить строку в соответствующий `*.tsv`, поставить
  `is_active=1`, запустить `ops/apply_catalogs.sh`.
- Изменить категорию: изменить строку в `*.tsv`, запустить
  `REBUILD_AGGREGATES=1 ops/apply_catalogs.sh`.
- Удалить категорию из отчетов: либо удалить строку из `*.tsv`, либо оставить
  строку для аудита и поставить `is_active=0`, затем запустить
  `REBUILD_AGGREGATES=1 ops/apply_catalogs.sh`.

`is_active=0` трактуется отчетами как `unknown`: запись остается видимой в
каталоге, но не используется для обогащения.

## Файлы

- `workstation_users.tsv` - привязка `host_name + user_login` к оргструктуре.
- `application_categories.tsv` - классификация desktop processes.
- `domain_categories.tsv` - классификация browser domains.

Формат: `TabSeparatedWithNames`, первая строка - имена колонок. Не используйте
tab-символы внутри значений.

## Таксономия РФ baseline

Baseline `catalog-ru-20260623` делит домены и приложения на рабочие для РФ
категории: `1c`, `edo_reporting`, `reporting`, `banking`, `government`,
`procurement`, `business_reference`, `legal_reference`, `mail`, `office`,
`pdf`, `browser`, `cloud_docs`, `communication`, `developer`, `admin_tool`,
`remote_admin`, `security_crypto`, `search`, `maps_reference`, `marketplace`,
`news`, `social`, `media`, `gaming`, `system`.

Правило ведения: справочник хранит только точные ключи. Для приложений это
нормализованный `process_name` в нижнем регистре, например `1cv8c.exe`; для
web - точный host из URL, например `online.sbis.ru`. Wildcard-строки не
используются, потому что ClickHouse Dictionary выполняет точный lookup.

`productivity_class` держите в одном из значений: `productive`, `neutral`,
`non_productive`, `unknown`. Для облаков, мессенджеров, AI и внешней почты
ставьте `risk_level=medium`, если нужна последующая DLP/policy проверка.
