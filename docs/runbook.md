# Runbook

## Быстрый health-check

### На Proxmox

```sh
pct status <CT_ID>
pct config <CT_ID>
pct exec <CT_ID> -- systemctl is-active activitywatch-server.service
pct exec <CT_ID> -- curl -fsS http://127.0.0.1:5600/api/0/info
```

### Внутри CT

```sh
systemctl status activitywatch-server.service --no-pager
journalctl -u activitywatch-server.service -n 100 --no-pager
curl -fsS http://127.0.0.1:5600/api/0/info
ss -ltnp | grep 5600
```

## Проверка RU patch

```sh
grep -n 'aw-ru-patch\|aw-sw-cleanup' /opt/activitywatch/webui-ru/index.html
ls -l /opt/activitywatch/webui-ru/js/
```

Проверить:

- есть `aw-ru-patch.js`;
- есть `aw-sw-cleanup.js`;
- `index.html` содержит оба include;
- `service-worker.js` заменён cleanup-версией.

## Типовые инциденты

### Сервис не стартует

```sh
systemctl cat activitywatch-server.service
cat /etc/activitywatch/aw-server.env
journalctl -xeu activitywatch-server.service --no-pager
```

Частые причины:

- битый URL релиза;
- неполная распаковка архива;
- занят порт;
- не созданы каталоги или пользователь;
- ошибка в env-файле.

### API отвечает, но UI без русификации

Проверить:

- патч реально вставлен в `index.html`;
- browser cache/service worker очищен;
- reverse proxy не отдаёт старую статику;
- сервис был перезапущен после правок.

Повторное применение:

```sh
bash /root/bootstrap/apply_webui_ru_patch.sh
systemctl restart activitywatch-server.service
```

### После обновления UI сломался патч

- сравнить `index.html` с backup;
- заново применить patch script;
- проверить словарь в `aw-ru-patch.js`;
- при необходимости откатить только Web UI override.

## Перед любыми изменениями

1. Сделать snapshot или `vzdump`.
2. Сохранить текущий `/etc/activitywatch/aw-server.env`.
3. Сохранить текущий `index.html`.
4. Зафиксировать текущую версию `aw-server-rust`.

## Критерии готовности

- systemd unit стартует без ручного вмешательства;
- API `/api/0/info` отвечает локально;
- UI открывается;
- русификация присутствует;
- rollback-путь понятен оператору.
