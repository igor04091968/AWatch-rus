# Worktime API and UI Bridge

## 2.4 Worktime API

`aw-server/aw-worktime-api.py` теперь рассчитан на повторные management-запросы без лишнего чтения одних и тех же bucket events.

## In-process events cache

Добавлен in-process cache событий:

```text
AW_WORKTIME_EVENTS_CACHE_TTL_SECONDS=30
```

Ключ cache - bucket id. Значение хранит `stored_at` и список events. Cache защищен `threading.Lock`, поэтому параллельные HTTP-запросы не ломают состояние процесса. Основной эффект: management report и trend building меньше давят на `/api/0/buckets/.../events`.

## Build locks для management report

Функция `get_management_build_lock(host, report_date)` выдает lock на пару `host + date`. Это предотвращает параллельную сборку одного и того же management report при одновременных запросах UI, warm-up и health probes.

Поведение:

- первый запрос собирает payload;
- конкурирующие запросы ждут тот же lock;
- после сборки результат кладется в cache/файловый слой как раньше.

## Оптимизация trend building

`build_management_trend(...)` принимает `precomputed_payloads`. Текущий report payload переиспользуется как готовый день тренда, а не пересчитывается повторно.

Практический эффект: endpoint management report меньше тратит CPU на день, который уже был рассчитан текущим запросом.

## UI Bridge foreground context cache

`aw-server/aw-worktime-ui-bridge.py` сохраняет `last_foreground_context` в state. Если свежий collector health event временно недоступен, bridge может использовать последний валидный foreground context вместо деградации в generic `RDP`.

Нормализация foreground context:

- `foregroundProcess` становится `app`;
- если процесс без `.exe`, suffix добавляется автоматически;
- `foregroundTitle` становится `title`;
- context учитывается только для активных session ids, если они известны.

## Active session detection

Функция `get_latest_active_session_ids(events)` группирует session events по timestamp, берет самый свежий срез и возвращает только активные `sessionId`.

Это убирает смешивание старых disconnected-сессий с текущим состоянием и стабилизирует связку:

```text
aw-worktime-sessions_* -> aw-rdp-window_* -> aw-watcher-window_*
```

## Нормализация window events

Bridge теперь формирует window events из session state и foreground context:

- активное окно получает app/title из foreground context;
- title дополняется агрегированным RDP summary через `build_window_title(...)`;
- `normalize_watcher_window_events(...)` подготавливает совместимый поток для `aw-watcher-window_<host>`;
- sync в watcher bucket выполняется только если `watcher_window_needs_bridge_sync(...)` считает его нужным.

Это сохраняет совместимость с ActivityWatch WebUI и Grafana, где часть запросов ожидает canonical watcher bucket.
