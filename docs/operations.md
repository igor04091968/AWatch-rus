# Operations

## Эксплуатационная модель

Система состоит из:

- узла Proxmox VE;
- LXC-контейнера с Debian 12;
- `ActivityWatch Server` на Rust;
- Web UI override с RU patch;
- DLP Web UI overlay для review/rules поверх bucket `aw-dlp-endpoint-signals_<HOST>`;
- systemd unit `activitywatch-server.service`.

Базовые правила:

- все изменения только через backup-first workflow;
- секреты не хранить в git;
- каждое изменение фиксировать в ticket/run log;
- публичную публикацию делать через отдельный proxy/security layer.
- generated-артефакты и исследовательские кэши вести по [artifacts-policy.md](/mnt/usb_hdd2/Projects/ActivityWatch-Russian/docs/artifacts-policy.md).

## Регулярные проверки

### Ежедневно или перед работами

- `pct status <CT_ID>`;
- `systemctl is-active activitywatch-server.service`;
- локальный `curl /api/0/info`;
- `df -h`;
- `journalctl -p err -b`.

### Еженедельно

- проверить свежесть backup/snapshot;
- проверить publish URL извне;
- проверить срок TLS, если есть reverse proxy;
- перепроверить firewall/ACL для `5600/tcp`.

### Ежемесячно

- проверить новые релизы ActivityWatch;
- сделать dry-run rollback;
- обновить runbook при изменениях инфраструктуры.

## Backup

Минимум сохранять:

- backup CT через `vzdump`;
- `/etc/activitywatch/aw-server.env`;
- `/etc/systemd/system/activitywatch-server.service`;
- `/opt/activitywatch/webui-ru/`;
- `/opt/activitywatch/releases/`;
- buckets `aw-dlp-review_*` и `aw-dlp-rules_*` через API export, если review/rule-классификация уже ведётся в UI;
- конфиг reverse proxy, если он есть.

Пример:

```sh
vzdump <CT_ID> --mode snapshot --compress zstd --storage <BACKUP_STORAGE>
pct exec <CT_ID> -- tar -C / -czf /root/activitywatch-config-backup.tgz \
  etc/activitywatch etc/systemd/system/activitywatch-server.service opt/activitywatch/webui-ru

curl -sS http://127.0.0.1:5600/api/0/buckets/aw-dlp-review_<HOST>/events?limit=500 > /root/aw-dlp-review-<HOST>.json
curl -sS http://127.0.0.1:5600/api/0/buckets/aw-dlp-rules_<HOST>/events?limit=500 > /root/aw-dlp-rules-<HOST>.json
```

## Rollback

### Быстрый rollback RU patch

```sh
cp /opt/activitywatch/webui-ru/index.html.bak.<timestamp> /opt/activitywatch/webui-ru/index.html
systemctl restart activitywatch-server.service
```

Примечание: после rollback или повторного деплоя открыть UI с hard refresh, так как браузер может держать старую версию `ru-patch-v5.js`.

### Rollback server release

1. Остановить сервис.
2. Переключить symlink на предыдущий release.
3. Проверить права.
4. Запустить сервис.
5. Проверить API/UI.

### Полный rollback CT

- остановить CT;
- восстановить snapshot или `vzdump`;
- поднять CT;
- проверить API и publish path.

## Обновление

Порядок:

1. Сделать backup.
2. Скачать новый release в отдельную директорию.
3. Не затирать прошлую версию до успешной проверки.
4. Проверить совместимость RU patch.
5. Перезапустить сервис.
6. Проверить API/UI.
7. Зафиксировать результат.

После обновления UI отдельно проверить:

- `#/home` — есть один корректный пункт `DLP`;
- `#/buckets/aw-dlp-endpoint-signals_<HOST>` — работают сохранение review/rule и списки `DLP Rules` / `DLP Review`;
- API создаёт/читает buckets `aw-dlp-review_<HOST>` и `aw-dlp-rules_<HOST>` без ошибок `304/409`.

## Эскалация

Эскалировать сразу, если:

- потерян доступ к Proxmox или CT;
- backup chain повреждён;
- UI/API не вернулись после rollback;
- изменились маршруты, bridge, VLAN или firewall policy;
- нужен новый публичный endpoint.

## Что не делать

- не вшивать реальные IP, пароли и токены;
- не обновлять поверх рабочего бинарника без backup;
- не открывать `5600/tcp` наружу без отдельной защиты;
- не править `index.html` вручную без backup.
- не запускать `scripts/prod_rollout.sh` без `AW_MAINTENANCE_ACK=YES`.
