# DetMir: Worktime (AQL templates)

Цель: получить «рабочее время» по данным ActivityWatch с учетом:

- активного времени (`not-afk`);
- категоризации (classes) для оконных событий;
- отдельного категоризованного веб-потока `aw-detmir-web-category_<HOST>` для доменов.

Ниже — шаблоны AQL для страницы `Query` в AW Web UI.

## Рабочее время по приложениям (окна)

Подходит для расчета рабочего времени в толстых клиентах (1С, документы, админка).

```javascript
events = flood(query_bucket("aw-watcher-window_SHARKON2025"));
not_afk = flood(query_bucket("aw-watcher-afk_SHARKON2025"));
not_afk = filter_keyvals(not_afk, "status", ["not-afk"]);

events = filter_period_intersect(events, not_afk);
events = categorize(events, __CATEGORIES__);

work = filter_keyvals(events, "$category", [
  ["Работа", "1С"],
  ["Работа", "Документы"],
  ["Работа", "Коммуникации"],
  ["Работа", "Администрирование"]
]);

work = merge_events_by_keys(work, ["$category", "app"]);
RETURN = sort_by_duration(work);
```

## Рабочее время в вебе (по доменам)

Требует, чтобы на клиенте работал browser collector и писал в:
`aw-detmir-web-category_<HOST>` поля `categoryGroup`, `rootDomain`.

```javascript
web = flood(query_bucket("aw-detmir-web-category_SHARKON2025"));
not_afk = flood(query_bucket("aw-watcher-afk_SHARKON2025"));
not_afk = filter_keyvals(not_afk, "status", ["not-afk"]);

web = filter_period_intersect(web, not_afk);
web = filter_keyvals(web, "categoryGroup", ["work"]);

web = merge_events_by_keys(web, ["rootDomain", "category"]);
RETURN = sort_by_duration(web);
```

## Sanity-check: «куда уходит время»

```javascript
events = flood(query_bucket("aw-watcher-window_SHARKON2025"));
not_afk = flood(query_bucket("aw-watcher-afk_SHARKON2025"));
not_afk = filter_keyvals(not_afk, "status", ["not-afk"]);

events = filter_period_intersect(events, not_afk);
events = categorize(events, __CATEGORIES__);

events = merge_events_by_keys(events, ["$category"]);
RETURN = sort_by_duration(events);
```

## Замечания

- Для других хостов замените суффикс `_SHARKON2025` на нужный hostname.
- Если web-поток пустой, рабочее время в браузере корректно посчитать по доменам не получится. Тогда либо:
  - чинить/запускать browser collector;
  - либо временно считать браузер в `window` как «Интернет/Браузер» без разделения на work/personal.
