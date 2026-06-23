# Источники ActivityWatch для workforce ingest: SHARKON2025

Дата проверки: `2026-06-23`

AW API: `http://10.10.10.13:5600/api/0`

Хост: `SHARKON2025`

## Итоговое решение P1

Основные источники для `aw_workforce`:

| Поток | Bucket | Решение | Поля |
|---|---|---|---|
| Desktop/window facts | `aw-watcher-window_SHARKON2025` | Загружать в `aw_window_events` | `app`, `hostname`, `processId`, `sessionId`, `source`, `title`, `username` |
| Browser facts, Edge | `aw-watcher-web-edge_SHARKON2025` | Загружать в `aw_browser_events` | `app`, `browser`, `hostname`, `sessionId`, `source`, `title`, `url`, `username` |
| Browser facts, Chrome | `aw-watcher-web-chrome_SHARKON2025` | Загружать как исторический browser source; если `username` отсутствует, писать `unknown` | `app`, `browser`, `sessionId`, `source`, `title`, `url` |

Не использовать как основной fact-source продуктивности:

| Bucket | Причина |
|---|---|
| `aw-rdp-window_SHARKON2025` | Есть активное окно RDP bridge, но нет `username` в событии; не годится для точной per-user привязки. |
| `aw-detmir-web-category_SHARKON2025` | Это health/category signal с `signalType=collector_health` и нулевой длительностью; полезен для диагностики collector/user presence, но не для длительности продуктивности. |
| `aw-worktime-sessions_SHARKON2025` | Авторитетный источник сессий и пользователей RDP, но это presence/session facts, а не window/browser usage facts. |

## Подтвержденные факты

`aw-watcher-window_SHARKON2025`:

- type: `currentwindow`;
- client: `aw-watcher-window`;
- hostname: `SHARKON2025`;
- за последние 24 часа есть события с пользователями `USER1`, `USER4`, `USER5`, `Администратор`;
- `USER1` подтвержден в этом bucket и будет нормализован loader-ом в `user1`.

`aw-watcher-web-edge_SHARKON2025`:

- type: `web.tab.current`;
- client: `aw-watcher-web-edge`;
- hostname: `SHARKON2025`;
- события за последние 30 дней содержат `username`;
- на момент проверки событий за последние 24 часа не было, поэтому источник включается как основной browser source, но freshness контролируется отдельно.

`aw-watcher-web-chrome_SHARKON2025`:

- type: `web.tab.current`;
- client: `aw-watcher-web-chrome`;
- hostname: `SHARKON2025`;
- последние найденные события исторические и не содержат `username`;
- loader загружает их с `user_login='unknown'`, пока нет надежной session correlation.

## Первая привязка P3

Файл загрузки: `clickhouse-workforce/sample/seed_sharkon2025_p3.sql`.

| host_name | user_domain | user_login | department | branch |
|---|---|---|---|---|
| `SHARKON2025` | `sharkon2025` | `user1` | `tsj` | `tsj` |

Ключ словаря остается `(host_name, user_login)`. Домен хранится как атрибут
`user_domain`, потому что текущие raw-события ActivityWatch дают `username`, а не
стабильный `DOMAIN\user` в window/browser facts.
