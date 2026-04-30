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

Для Linux-удалёнщиков это может быть не URL-level collector, а title/class-based web-category logger.
Например, работа через Proxmox Web UI `https://...:8006` может попадать сюда как
`rootDomain=proxmox-webui`, `categoryGroup=work`, `category=Администрирование`.

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

## Presence по удалёнщикам Windows/RDP

Если на Windows-клиенте развернут `worktime-session-collector.ps1`, то появляется bucket
`aw-worktime-sessions_<HOST>` с heartbeat по `quser`/RDP session state.

Это не замена `afk/window`, а отдельный канал для ответа на вопрос:
«кто и когда вообще был в активной удалённой сессии».

```javascript
sessions = flood(query_bucket("aw-worktime-sessions_SHARKON2025"));
sessions = filter_keyvals(sessions, "active", [true]);
sessions = merge_events_by_keys(sessions, ["username", "sessionName", "state"]);
RETURN = sort_by_duration(sessions);
```

Практический смысл:

- для GUI-удалёнщиков рабочее время лучше считать по пересечению `window` + `not-afk`;
- для RDP presence и быстрой сверки смены можно использовать `aw-worktime-sessions_*`;
- для SSH-only пользователей нужны `aw-console-commands_*` и `aw-ssh-sessions_*`, но это не полный аналог desktop worktime.
